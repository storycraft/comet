pub mod components;
pub mod fragment;
pub mod input;
pub mod layer;
pub mod resolver;
mod taffy;
pub mod tree;

use ::taffy::AvailableSpace;
use kurbo::{Point, Rect, Size};
use parley::{ClusterPath, FontContext};
use slotmap::new_key_type;

use crate::{
    layout::{
        taffy::{TaffyLayoutImpl, to_taffy_key},
        tree::LayoutBoxTree,
    },
    ui::{NodeKey, Ui},
};

pub type ContainerLayoutFn = fn();

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(transparent)]
pub struct ContainerLayout(ContainerLayoutFn);

impl ContainerLayout {
    pub const fn new(f: ContainerLayoutFn) -> Self {
        Self(f)
    }

    pub fn compute_layout(self) {
        // TODO
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoxLayout {
    /// Relative z-index of the layout box
    pub z_index: u32,

    /// Top-left location of the layout box
    pub location: Point,

    /// Bounding box size of the layout box
    pub size: Size,

    /// Rectangle representing the content area of the layout box.
    /// It can be larger than `size` when content overflows.
    pub content_size: Size,

    /// The size of the borders of the layout box
    pub border: Rect,

    /// The size of the padding of the layout box
    pub padding: Rect,

    /// The size of the margin of the layout box
    pub margin: Rect,
}

impl BoxLayout {
    pub fn new() -> Self {
        Self {
            z_index: 0,
            location: Point::ORIGIN,
            size: Size::ZERO,
            content_size: Size::ZERO,
            border: Rect::ZERO,
            padding: Rect::ZERO,
            margin: Rect::ZERO,
        }
    }

    fn from_taffy_layout(layout: ::taffy::Layout) -> BoxLayout {
        #[inline]
        fn to_kurbo_rect(r: ::taffy::Rect<f32>) -> Rect {
            Rect::new(
                r.left as f64,
                r.top as f64,
                r.left as f64 + r.right as f64,
                r.top as f64 + r.bottom as f64,
            )
        }

        #[inline]
        fn to_kurbo_point(p: ::taffy::Point<f32>) -> Point {
            Point::new(p.x as f64, p.y as f64)
        }

        #[inline]
        fn to_kurbo_size(size: ::taffy::Size<f32>) -> Size {
            Size::new(size.width as f64, size.height as f64)
        }

        Self {
            z_index: layout.order,
            location: to_kurbo_point(layout.location),
            size: to_kurbo_size(layout.size),
            content_size: to_kurbo_size(layout.size),
            border: to_kurbo_rect(layout.border),
            padding: to_kurbo_rect(layout.padding),
            margin: to_kurbo_rect(layout.margin),
        }
    }
}

impl Default for BoxLayout {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct LayoutBox {
    pub span: Option<NodeKey>,
    pub(crate) taffy_cache: ::taffy::Cache,
    pub layout: BoxLayout,

    pub ty: LayoutTy,
}

impl LayoutBox {
    pub fn new(span: Option<NodeKey>, ty: LayoutTy) -> Self {
        Self {
            span,
            taffy_cache: ::taffy::Cache::new(),
            layout: BoxLayout::new(),

            ty,
        }
    }

    pub fn invalidated(&self) -> bool {
        self.taffy_cache.is_empty()
    }

    pub fn invalidate(&mut self) {
        self.taffy_cache.clear();
        self.layout = BoxLayout::new();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutTy {
    Block,
    Inline(InlineBoxKey),
}

new_key_type! {
    pub struct LayoutBoxKey;
    pub struct InlineBoxKey;
    pub struct InlineKey;
}

pub struct InlineBox {
    pub parley_layout: parley::Layout<()>,
    pub inline_start: Option<InlineKey>,
    pub texts: String,
}

impl InlineBox {
    pub fn new() -> Self {
        Self {
            parley_layout: parley::Layout::new(),
            inline_start: None,
            texts: String::new(),
        }
    }
}

impl Default for InlineBox {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InlineIns {
    /// A text with length
    Text(usize),
    /// Push new inline box
    PushInlineBox(NodeKey),
    /// Pop inline box
    PopInlineBox,
    /// A new layout box
    Box(LayoutBoxKey),
}

pub struct LayoutContext {
    parley: parley::LayoutContext<()>,
    inline_states: Vec<InlineState>,
}

impl LayoutContext {
    pub fn new() -> Self {
        Self {
            parley: parley::LayoutContext::new(),
            inline_states: vec![],
        }
    }

    pub fn layout<'a>(
        &mut self,
        font_cx: &'a mut FontContext,
        ui: &'a Ui,
        tree: &'a mut LayoutBoxTree,
        root: LayoutBoxKey,
        available_space: ::taffy::Size<AvailableSpace>,
    ) {
        ::taffy::compute::compute_root_layout(
            &mut TaffyLayoutImpl {
                font_cx,
                cx: self,
                ui,
                tree,
            },
            to_taffy_key(root),
            available_space,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct InlineState {
    pub start: ClusterPath,
    pub start_inline: InlineBoxKey,
    pub span: NodeKey,
}

pub struct LayoutLineBox {}
