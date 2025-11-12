mod taffy;
pub mod tree;
pub mod components;

use kurbo::{Point, Rect, Size};

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayOuter {
    #[default]
    /// Element generates a block layout box.
    Block,
    /// Element is part of inline content.
    Inline,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum DisplayInner {
    #[default]
    /// Generate layout boxes and display children using Flow layout.
    Flow,
    /// Establish a new flow context and layout children inside.
    FlowRoot,
    /// Layout each children with given layout algorithm.
    Container(ContainerLayout),
    /// Display a content inside. Children will not be laid out.
    Content,
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
