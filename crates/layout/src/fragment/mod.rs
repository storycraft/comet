//! Box fragment tree with flattened inline layouts.
//!
//! Only boxes are tree nodes. Each inline formatting context is held by the
//! box establishing it as one [`InlineLayout`]: its text, Parley layout and a
//! flat, preorder list of [`InlineItem`](inline::InlineItem)s for lines, inline element pieces and
//! glyph runs. Block level boxes inside inline content split it into runs:
//! the box establishing it lays them out with the block algorithm, each run
//! in an anonymous box with an inline layout of its own. The run boxes are
//! linked by [`BoxFragmentLink`], so an inline element split by a block is
//! followed across them.

pub mod builder;
pub mod inline;
pub mod tree;

#[cfg(test)]
mod tests;

use bitflags::bitflags;
use slotmap::new_key_type;

use inline::InlineLayout;

use crate::{input::LayoutInputKey, taffy::style::Style};
use comet_geometry::{Point, Rect, Size, StyleUnit};

new_key_type! {
    pub struct BoxFragmentKey;
}

/// How a box takes part in the layout of its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoxFragmentType {
    /// In-flow block level box, a child of a container, or a build root.
    Normal,
    /// Anonymous in-flow block level box generated for its source, wrapping
    /// content in an inline formatting context, and painting nothing of its
    /// own: a run of the inline content of a
    /// [mixed inline](ChildLayoutType::MixedInline) box, which is not the box
    /// of its source, or the box of a text item where a box is needed, as a
    /// build root or a child of a container. Text generates no box of its
    /// own.
    Anonymous,
    /// In-flow block level box establishing a new block formatting context,
    /// such as `display: flow-root`. Its descendants' margins and floats do
    /// not interact with those outside it. Inline level flow roots are
    /// [`Self::AtomicInline`].
    FlowRoot,
    /// Piece of an inline element on one line of the inline layout of its
    /// parent box, decorating the content it holds. An anonymous box, linked
    /// to the other pieces of the element by [`BoxFragmentLink`].
    InlinePart,
    /// Atomic inline box, placed by a line of the inline layout of its parent
    /// box.
    AtomicInline,
    /// Out-of-flow box, placed by its containing block.
    OutOfFlow,
    /// Floating box, taken out of flow and placed at the side of its block
    /// formatting context, with inline content flowing around it. Not
    /// generated yet.
    Floating,
}

/// How a box lays out its content.
#[derive(Debug)]
pub enum ChildLayoutType {
    /// Child boxes laid out by the block algorithm.
    Block,
    /// Inline content laid out as one inline layout. Child boxes are the
    /// atomic inline and out-of-flow boxes inside it, in flow order.
    Inline(Box<InlineLayout>),
    /// Inline content split by block level boxes, laid out by the block
    /// algorithm. Child boxes are, in flow order, the block level boxes and
    /// an anonymous box for each run of inline content between them, which
    /// lays the run out as [`Self::Inline`]. Holds the first run, from which
    /// the others follow by their [`BoxFragmentLink`]s.
    MixedInline(Option<BoxFragmentKey>),
    /// Child boxes laid out by a custom container algorithm.
    Container,
    /// No children are laid out.
    Content,
}

impl ChildLayoutType {
    pub const fn is_block(&self) -> bool {
        matches!(self, Self::Block)
    }

    pub const fn is_inline(&self) -> bool {
        matches!(self, Self::Inline(_))
    }

    pub const fn is_mixed_inline(&self) -> bool {
        matches!(self, Self::MixedInline(_))
    }

    /// Whether child boxes are laid out by the block algorithm.
    pub const fn is_block_container(&self) -> bool {
        matches!(self, Self::Block | Self::MixedInline(_))
    }

    /// The first run box of mixed inline content.
    pub const fn first_run(&self) -> Option<BoxFragmentKey> {
        match *self {
            Self::MixedInline(first) => first,
            _ => None,
        }
    }

    pub const fn is_container(&self) -> bool {
        matches!(self, Self::Container)
    }

    pub const fn is_content(&self) -> bool {
        matches!(self, Self::Content)
    }

    /// The inline layout of an inline formatting context.
    pub fn inline(&self) -> Option<&InlineLayout> {
        match self {
            Self::Inline(inline) => Some(inline),
            _ => None,
        }
    }

    pub fn inline_mut(&mut self) -> Option<&mut InlineLayout> {
        match self {
            Self::Inline(inline) => Some(inline),
            _ => None,
        }
    }
}

/// Computed geometry of a box fragment or an inline item.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct FragmentGeometry {
    /// Coordinates relative to the parent box fragment's border box.
    pub location: Point<f64>,
    pub size: Size<f64>,
    pub padding: Rect<f64>,
    pub border: Rect<f64>,
    /// Layout overflow: the area covered by the border box and by the
    /// overflow of every descendant, in local coordinates with the border
    /// box's top-left corner at the origin. Descendants clipped by a box
    /// clipping its overflow only contribute that box's border box.
    pub overflow: kurbo::Rect,
}

impl FragmentGeometry {
    /// The border box, in local coordinates.
    pub fn border_box(&self) -> kurbo::Rect {
        kurbo::Rect::new(0.0, 0.0, self.size.width, self.size.height)
    }

    /// The area covered by the fragment and its descendants, in the local
    /// coordinates of its parent: its [overflow](Self::overflow), placed at
    /// its location.
    pub fn extent(&self) -> kurbo::Rect {
        self.overflow + kurbo::Vec2::new(self.location.x, self.location.y)
    }
}

/// Layout results of a box fragment.
#[derive(Debug, Default, Clone)]
pub struct BoxFragmentLayout {
    /// Geometry used by later steps, such as painting.
    pub geometry: FragmentGeometry,
    /// Unrounded layout of the latest final layout pass.
    pub unrounded: taffy::Layout,
    /// Baseline relative to the border box top, for atomic inlines and
    /// roots.
    pub baseline: Option<f32>,
}

/// Links the box fragments split from one source, in flow order: the
/// anonymous run boxes of a [mixed inline](ChildLayoutType::MixedInline) box,
/// the [pieces](BoxFragmentType::InlinePart) of an inline element across
/// lines and runs, and later boxes broken across fragmentainers.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoxFragmentLink {
    pub prev: Option<BoxFragmentKey>,
    pub next: Option<BoxFragmentKey>,
}

/// A box generated by a layout input item. Box fragments keep their keys
/// across builds.
#[derive(Debug)]
pub struct BoxFragment {
    /// Item generating the box. An anonymous box, one that is not the box
    /// of its source in its [`FragmentTree`](tree::FragmentTree), belongs to the item it was
    /// generated for, such as the box whose content it wraps.
    pub source: LayoutInputKey,
    pub box_type: BoxFragmentType,
    pub child_layout: ChildLayoutType,
    pub layout: BoxFragmentLayout,
    /// The other boxes split from the same source.
    pub link: BoxFragmentLink,
    pub(crate) style: Style,
    pub(crate) cache: taffy::Cache,
    /// Whether retained descendant geometry and inline items match the
    /// final cache entry.
    pub(crate) final_layout_valid: bool,
    /// What the layout of this box depends on, with the
    /// [propagated](NodeLayoutBits::PROPAGATED) bits of its subtree.
    pub(crate) bits: NodeLayoutBits,
    /// Out-of-flow boxes this box is the containing block of.
    pub(crate) hoisted: Vec<BoxFragmentKey>,
}

impl BoxFragment {
    pub fn new(
        source: LayoutInputKey,
        box_type: BoxFragmentType,
        child_layout: ChildLayoutType,
        style: Style,
    ) -> Self {
        Self {
            source,
            box_type,
            child_layout,
            layout: BoxFragmentLayout::default(),
            link: BoxFragmentLink::default(),
            style,
            cache: taffy::Cache::new(),
            final_layout_valid: false,
            bits: NodeLayoutBits::empty(),
            hoisted: Vec::new(),
        }
    }

    pub fn style(&self) -> &Style {
        &self.style
    }

    pub fn bits(&self) -> NodeLayoutBits {
        self.bits
    }

    /// The inline layout of the inline formatting context this box
    /// establishes.
    pub fn inline(&self) -> Option<&InlineLayout> {
        self.child_layout.inline()
    }

    pub fn inline_mut(&mut self) -> Option<&mut InlineLayout> {
        self.child_layout.inline_mut()
    }

    /// Drop cached layouts, keeping the retained geometry.
    pub(crate) fn invalidate(&mut self) {
        self.cache.clear();
        self.final_layout_valid = false;
    }
}

/// A fragment of a [`FragmentTree`](tree::FragmentTree): a box, or an item of the inline layout
/// of a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FragmentRef {
    Box(BoxFragmentKey),
    Inline { owner: BoxFragmentKey, index: u32 },
}

bitflags! {
    /// What the layout of a box or of its subtree depends on, besides its
    /// own content. Set when box styles are resolved and from what custom
    /// container algorithms return. [`Self::PROPAGATED`] bits are also set
    /// on every ancestor box up to the root of the build.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    pub struct NodeLayoutBits: u8 {
        /// A length of the box itself resolves against its parent's size,
        /// such as a percentage. Not propagated: a box whose size is known
        /// lays out its subtree the same whatever its parent's size, since
        /// its descendants resolve against it or boxes inside it, and
        /// out-of-flow descendants claimed by an ancestor are laid out by
        /// that ancestor.
        const DEPENDS_PARENT_SIZE = 1 << 0;
        /// A length uses `vw` or `vh`.
        const DEPENDS_VIEWPORT = 1 << 1;
    }
}

impl NodeLayoutBits {
    /// Bits set on the ancestors of a box too, so a build finds the box
    /// from the root.
    pub const PROPAGATED: Self = Self::DEPENDS_VIEWPORT;

    /// What a length of `unit` depends on.
    pub fn of_unit(unit: StyleUnit) -> Self {
        match unit {
            StyleUnit::Percent(_) => Self::DEPENDS_PARENT_SIZE,
            unit if unit.is_viewport_relative() => Self::DEPENDS_VIEWPORT,
            _ => Self::empty(),
        }
    }
}
