//! Chainable style helpers.
//!
//! Every helper takes `impl Into<Signal<T>>`: a plain value, a signal, a memo
//! or a [`Signal::derive`] closure, and binds the matching prop with a render
//! effect, so the prop follows the value.
//!
//! ```ignore
//! cx.div()
//!     .padding(all(px(8)))
//!     .fill(Signal::derive(move || if hovered.get() { dark } else { light }))
//!     .build()
//! ```

use anyrender::Paint;
use comet_layout::style::{
    bx,
    geometry::{AutoValue, Point, Rect, Size, StyleUnit},
    parley, taffy,
};
use comet_layout_flex::{flex, style as flex_style};
use comet_paint::style as paint;
use reactive_graph::{traits::Get, wrappers::read::Signal};

use crate::{cx::NodeBuild, prop::ReactiveProp};

/// Generate helpers binding `$prop` from a `Signal<$value>`.
macro_rules! style_helpers {
    ($($(#[$attr:meta])* $name:ident: $prop:ty => $value:ty;)*) => {$(
        $(#[$attr])*
        fn $name(self, value: impl Into<Signal<$value>>) -> Self {
            self.bind::<$prop, $value>(value)
        }
    )*};
}

/// Style helpers of a node being built, for the box, text and paint props.
///
/// Implement [`Self::bind`] to get every helper; other crates add helpers for
/// their own props with extension traits over `Styled`, like [`FlexStyled`].
pub trait Styled: Sized {
    /// Bind prop `P` to `value`, converting each value with `P::from`.
    fn bind<P, T>(self, value: impl Into<Signal<T>>) -> Self
    where
        P: ReactiveProp + From<T>,
        T: Clone + Send + Sync + 'static;

    style_helpers! {
        // Display
        display_outer: bx::DisplayOuter => bx::DisplayOuter;
        display_inner: bx::DisplayInner => bx::DisplayInner;

        // Position
        position: bx::Position => taffy::Position;
        inset: bx::Inset => Rect<AutoValue<StyleUnit>>;

        // Size
        size: bx::Size => Size<AutoValue<StyleUnit>>;
        min_size: bx::MinSize => Size<AutoValue<StyleUnit>>;
        max_size: bx::MaxSize => Size<AutoValue<StyleUnit>>;
        aspect_ratio: bx::AspectRatio => f32;
        overflow: bx::Overflow => Point<taffy::Overflow>;
        box_sizing: bx::BoxSizing => taffy::BoxSizing;

        // Spacing
        margin: bx::Margin => Rect<AutoValue<StyleUnit>>;
        padding: bx::Padding => Rect<StyleUnit>;

        // Border
        border: bx::Border => Rect<StyleUnit>;
        border_join: bx::BorderJoin => kurbo::Join;
        border_miter_limit: bx::BorderMiterLimit => StyleUnit;
        border_start_cap: bx::BorderStartCap => kurbo::Cap;
        border_end_cap: bx::BorderEndCap => kurbo::Cap;
        border_dash_pattern: bx::BorderDashPattern => kurbo::Dashes;
        border_dash_offset: bx::BorderDashOffset => StyleUnit;
        border_radius: paint::BorderRadius => Rect<StyleUnit>;

        // Text
        font: bx::Font => parley::FontFamily<'static>;
        font_size: bx::FontSize => StyleUnit;
        font_variations: bx::FontVariations => parley::FontVariations<'static>;
        font_features: bx::FontFeatures => parley::FontFeatures<'static>;
        font_style: bx::FontStyle => parley::FontStyle;
        font_weight: bx::FontWeight => parley::FontWeight;
        locale: bx::Locale => parley::Language;
        underline_offset: bx::UnderlineOffset => StyleUnit;
        underline_size: bx::UnderlineSize => StyleUnit;
        strikethrough_offset: bx::StrikethroughOffset => StyleUnit;
        strikethrough_size: bx::StrikethroughSize => StyleUnit;
        line_height: bx::LineHeight => StyleUnit;
        word_break: bx::WordBreak => parley::WordBreak;
        word_spacing: bx::WordSpacing => StyleUnit;
        letter_spacing: bx::LetterSpacing => StyleUnit;
        text_align: bx::TextAlign => parley::Alignment;
        text_color: paint::TextColor => Paint;

        // Paint
        fill: paint::Fill => Paint;
        border_fill: paint::BorderFill => Paint;
        outline_fill: paint::OutlineFill => Paint;
        outline_width: paint::OutlineWidth => StyleUnit;
        outline_offset: paint::OutlineOffset => StyleUnit;
        transform: paint::Transform => kurbo::Affine;
        transform_origin: paint::TransformOrigin => Point<StyleUnit>;
        z_index: paint::ZIndex => AutoValue<i32>;
        opacity: paint::Opacity => f64;
        filter: paint::Filter => Vec<anyrender::filters::FilterEffect>;
        backdrop_filter: paint::BackdropFilter => Vec<anyrender::filters::FilterEffect>;
        mix_blend: paint::MixBlend => peniko::BlendMode;
        scroll_left: paint::ScrollLeft => f64;
        scroll_top: paint::ScrollTop => f64;

        // Accessibility.
        role: comet_accesskit::style::Role => accesskit::Role;
    }
}

/// Flexbox helpers: [`flex`](Self::flex) to lay out the children as a flex
/// container, and the [`comet_layout_flex::styles`] props.
pub trait FlexStyled: Styled {
    /// Lay out the children with flexbox.
    fn flex(self) -> Self {
        self.display_inner(bx::DisplayInner::Container(flex()))
    }

    style_helpers! {
        flex_direction: flex_style::FlexDirection => flex_style::taffy::FlexDirection;
        flex_wrap: flex_style::FlexWrap => flex_style::taffy::FlexWrap;
        flex_line_count: flex_style::FlexLineCount => u16;
        gap: flex_style::Gap => Size<StyleUnit>;
        align_items: flex_style::AlignItems => flex_style::taffy::AlignItems;
        align_content: flex_style::AlignContent => flex_style::taffy::AlignContent;
        justify_content: flex_style::JustifyContent => flex_style::taffy::JustifyContent;
        flex_grow: flex_style::FlexGrow => f32;
        flex_shrink: flex_style::FlexShrink => f32;
        flex_basis: flex_style::FlexBasis => AutoValue<StyleUnit>;
        align_self: flex_style::AlignSelf => Option<flex_style::taffy::AlignSelf>;
    }
}

impl<S: Styled> FlexStyled for S {}

impl Styled for NodeBuild<'_> {
    fn bind<P, T>(self, value: impl Into<Signal<T>>) -> Self
    where
        P: ReactiveProp + From<T>,
        T: Clone + Send + Sync + 'static,
    {
        let value = value.into();
        self.style_with(move || P::from(value.get()))
    }
}
