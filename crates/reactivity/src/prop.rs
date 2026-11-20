//! Which pipeline steps a prop change invalidates.

use bitflags::bitflags;
use comet_accesskit::style as a11y;
use comet_layout::{dirty_map::DirtyFlags, style::bx};
use comet_layout_flex::styles as flex;
use comet_paint::style as paint;
use comet_tree::{node::Text, style::StyleProp};

bitflags! {
    /// What a change of a node invalidates, from the steps that read it.
    ///
    /// Layout flags include [`Self::RENDER`]: whatever is laid out or
    /// pre-painted again is painted again. They do not include
    /// [`Self::A11Y`]: the accessibility build follows the layout's own
    /// flags, so it is only for changes the layout does not see.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct PropFlags: u8 {
        /// Painted again.
        const RENDER = 1 << 0;
        /// Its fragments are pre-painted again.
        const PRE_PAINT = 1 << 1 | Self::RENDER.bits();
        /// Its box is laid out again.
        const BOX_LAYOUT = 1 << 2 | Self::RENDER.bits();
        /// Its children changed: they are built and laid out again.
        const CHILDREN = 1 << 3 | Self::RENDER.bits();
        /// Its layout input is resolved and laid out again.
        const FULL_LAYOUT = 1 << 4 | Self::BOX_LAYOUT.bits() | Self::CHILDREN.bits();
        /// Its accessibility nodes are resolved again.
        const A11Y = 1 << 5;
    }
}

impl PropFlags {
    /// The flags to mark on the node's layout item. Empty when the layout
    /// has nothing to do.
    pub const fn layout(self) -> DirtyFlags {
        let mut flags = DirtyFlags::empty();
        if self.contains(Self::PRE_PAINT) {
            flags = flags.union(DirtyFlags::NEED_PRE_PAINT);
        }
        if self.contains(Self::BOX_LAYOUT) {
            flags = flags.union(DirtyFlags::NEED_BOX_LAYOUT);
        }
        if self.contains(Self::CHILDREN) {
            flags = flags.union(DirtyFlags::NEED_CHILDREN);
        }
        if self.contains(Self::FULL_LAYOUT) {
            flags = flags.union(DirtyFlags::NEED_FULL_LAYOUT);
        }
        flags
    }
}

/// A prop that can be bound to a reactive value.
///
/// Implement it for custom props, choosing the flags of the first step of
/// each kind that reads the prop. Later layout steps follow from it: a laid
/// out item is pre-painted again, and the pre-paint step marks layers itself.
pub trait ReactiveProp: StyleProp + PartialEq {
    /// Flags recorded for the node when the prop changes.
    const DIRTY: PropFlags;
}

const FULL: PropFlags = PropFlags::FULL_LAYOUT;
const BOX: PropFlags = PropFlags::BOX_LAYOUT;
const PRE_PAINT: PropFlags = PropFlags::PRE_PAINT;
const REPAINT: PropFlags = PropFlags::RENDER;
const A11Y: PropFlags = PropFlags::A11Y;

macro_rules! reactive_props {
    ($flags:expr => $($ty:ty),* $(,)?) => {
        $(impl ReactiveProp for $ty {
            const DIRTY: PropFlags = $flags;
        })*
    };
}

// Resolved by the input builder: display and inherited text styles. Text
// content is a content change, which shapes the inline flow again.
reactive_props!(FULL =>
    Text,
    bx::DisplayOuter,
    bx::DisplayInner,
    bx::Font,
    bx::FontSize,
    bx::FontVariations,
    bx::FontFeatures,
    bx::FontStyle,
    bx::FontWeight,
    bx::Locale,
    bx::UnderlineOffset,
    bx::UnderlineSize,
    bx::StrikethroughOffset,
    bx::StrikethroughSize,
    bx::LineHeight,
    bx::WordBreak,
    bx::WordSpacing,
    bx::LetterSpacing,
    bx::TextAlign,
);

// Read by the fragment builder.
reactive_props!(BOX =>
    bx::Position,
    bx::Inset,
    bx::Size,
    bx::MinSize,
    bx::MaxSize,
    bx::AspectRatio,
    bx::BoxSizing,
    bx::Margin,
    bx::Padding,
    bx::Border,
    bx::BorderJoin,
    bx::BorderMiterLimit,
    bx::BorderStartCap,
    bx::BorderEndCap,
    bx::BorderDashPattern,
    bx::BorderDashOffset,
    flex::FlexDirection,
    flex::FlexWrap,
    flex::FlexLineCount,
    flex::Gap,
    flex::AlignItems,
    flex::AlignContent,
    flex::JustifyContent,
    flex::FlexGrow,
    flex::FlexShrink,
    flex::FlexBasis,
    flex::AlignSelf,
);

// Affects both the layout and the clip resolved by the pre-paint step.
reactive_props!(BOX.union(PRE_PAINT) => bx::Overflow);

// Resolved by the pre-paint step, which also decides stacking.
reactive_props!(PRE_PAINT =>
    paint::Transform,
    paint::TransformOrigin,
    paint::BorderRadius,
    paint::OutlineWidth,
    paint::OutlineOffset,
    paint::ZIndex,
    paint::Opacity,
    paint::Filter,
    paint::BackdropFilter,
);

// Read by the painter every frame.
reactive_props!(REPAINT =>
    paint::Fill,
    paint::BorderFill,
    paint::OutlineFill,
    paint::TextColor,
);

// Read by the accessibility build only.
reactive_props!(A11Y => a11y::Role);
