//! Which pipeline steps a prop change invalidates.

use comet_accesskit::style as a11y;
use comet_layout::{dirty_map::DirtyFlags, style::bx};
use comet_layout_flex::styles as flex;
use comet_paint::style as paint;
use comet_tree::{node::Text, style::StyleProp};

/// A prop that can be bound to a reactive value.
///
/// Implement it for custom props, choosing the flags of the first step that
/// reads the prop. Later steps follow from it: a laid out item is pre-painted
/// again, and the pre-paint step marks layers itself.
pub trait ReactiveProp: StyleProp + PartialEq {
    /// Flags marked on the node's layout item when the prop changes.
    /// Empty when only a repaint is needed.
    const DIRTY: DirtyFlags;
}

const FULL: DirtyFlags = DirtyFlags::NEED_FULL_LAYOUT;
const BOX: DirtyFlags = DirtyFlags::NEED_BOX_LAYOUT;
const PRE_PAINT: DirtyFlags = DirtyFlags::NEED_PRE_PAINT;
const REPAINT: DirtyFlags = DirtyFlags::empty();

macro_rules! reactive_props {
    ($flags:expr => $($ty:ty),* $(,)?) => {
        $(impl ReactiveProp for $ty {
            const DIRTY: DirtyFlags = $flags;
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

// Read by the accessibility build, which layout flags do not reach yet: a
// change is written into the tree, but the accessibility tree keeps the old
// value until something else resolves the node again.
reactive_props!(DirtyFlags::empty() => a11y::Role);
