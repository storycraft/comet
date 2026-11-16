//! Inherited text styles of input items, resolved for Parley inline layout.

use std::sync::Arc;

use bitflags::bitflags;
use comet_tree::ui::Props;

use super::LayoutInputKey;
use crate::style::{
    bx,
    geometry::{StyleUnit, UnitResolveContext},
    parley::Alignment,
};

/// Parley style of a text span. Its brush is set while shaping to the item
/// owning the span, so glyph runs map back to their item.
pub type SpanStyle = ::parley::TextStyle<'static, 'static, Option<LayoutInputKey>>;

bitflags! {
    /// What the resolved [`TextStyle`] of an item, or of its descendants,
    /// depends on. Kept on the item, apart from the shared style.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    pub struct TextStyleBits: u8 {
        /// A length of the style uses `vw` or `vh`, so the style resolves
        /// differently when the viewport size changes. Inherited from the
        /// parent, since inherited values were resolved with it.
        const DEPENDS_VIEWPORT = 1 << 0;
        /// A descendant item is [`Self::DEPENDS_VIEWPORT`]. Set on every
        /// ancestor of such an item, up to the root. Only added, so it may
        /// outlive the descendants that set it.
        const CHILD_DEPENDS_VIEWPORT = 1 << 1;
    }
}

/// Inherited text properties of an item: the style of its text spans and the
/// alignment of the lines in an inline formatting context it establishes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextStyle {
    pub span: SpanStyle,
    pub align: Alignment,
}

/// Default style for a root without a source parent. Its font size is the root font size.
pub fn root_text_style(root_font_size: f64) -> TextStyle {
    TextStyle {
        span: SpanStyle {
            font_size: root_font_size as f32,
            ..SpanStyle::default()
        },
        ..TextStyle::default()
    }
}

/// Resolve a text length. Percentages and `em` use `cx.font_size`.
fn resolve_length(value: StyleUnit, cx: &UnitResolveContext, percentage_basis: f64) -> f32 {
    value.resolve(cx, percentage_basis) as f32
}

/// Overlay explicit properties on the parent's computed style, including the
/// span style and the text alignment.
///
/// `cx` supplies the root font size and viewport; its `font_size` is
/// overwritten. `font-size` resolves `em` and percentages against the parent's
/// font size, while every other length resolves against the resolved font size.
/// The parent allocation is shared when no property changes the style.
///
/// Returns the style with its [`TextStyleBits::DEPENDS_VIEWPORT`] bit, set
/// when a length uses `vw` or `vh` or when `parent_bits` has it.
pub fn resolve_text_style(
    props: Props,
    cx: &UnitResolveContext,
    parent: &Arc<TextStyle>,
    parent_bits: TextStyleBits,
) -> (Arc<TextStyle>, TextStyleBits) {
    let mut style = parent.clone();
    let mut cx = UnitResolveContext {
        font_size: style.span.font_size as f64,
        ..cx.clone()
    };
    // Every inherited value was resolved with the parent's dependencies.
    let mut bits = parent_bits & TextStyleBits::DEPENDS_VIEWPORT;
    let mut length = |value: StyleUnit, cx: &UnitResolveContext, percentage_basis: f64| {
        if value.is_viewport_relative() {
            bits |= TextStyleBits::DEPENDS_VIEWPORT;
        }

        resolve_length(value, cx, percentage_basis)
    };

    macro_rules! set {
        ($($field:ident).+, $value:expr) => {
            let value = $value;
            if style.$($field).+ != value {
                Arc::make_mut(&mut style).$($field).+ = value;
            }
        };
    }

    // `font-size` percentages use the parent's font size.
    if let Some(value) = props.get::<bx::FontSize>() {
        set!(span.font_size, length(value.0, &cx, cx.font_size));
    }

    // Every other length resolves against the resolved font size.
    cx.font_size = style.span.font_size as f64;
    let percentage_basis = cx.font_size;

    macro_rules! inherit {
        ($prop:ident, $field:ident) => {
            if let Some(value) = props.get::<bx::$prop>() {
                if style.span.$field != value.0 {
                    Arc::make_mut(&mut style).span.$field = value.0.clone();
                }
            }
        };
        ($prop:ident, $field:ident, length) => {
            if let Some(value) = props.get::<bx::$prop>() {
                set!(span.$field, length(value.0, &cx, percentage_basis));
            }
        };
        ($prop:ident, $field:ident, optional) => {
            if let Some(value) = props.get::<bx::$prop>() {
                set!(span.$field, Some(length(value.0, &cx, percentage_basis)));
            }
        };
    }

    inherit!(Font, font_family);
    inherit!(FontVariations, font_variations);
    inherit!(FontFeatures, font_features);
    inherit!(FontStyle, font_style);
    inherit!(FontWeight, font_weight);
    inherit!(WordBreak, word_break);
    inherit!(WordSpacing, word_spacing, length);
    inherit!(LetterSpacing, letter_spacing, length);
    inherit!(UnderlineOffset, underline_offset, optional);
    inherit!(UnderlineSize, underline_size, optional);
    inherit!(StrikethroughOffset, strikethrough_offset, optional);
    inherit!(StrikethroughSize, strikethrough_size, optional);

    if let Some(value) = props.get::<bx::Locale>() {
        set!(span.locale, Some(value.0));
    }
    if let Some(value) = props.get::<bx::LineHeight>() {
        set!(
            span.line_height,
            parley::LineHeight::Absolute(length(value.0, &cx, percentage_basis))
        );
    }
    if let Some(value) = props.get::<bx::TextAlign>() {
        set!(align, value.0);
    }

    (style, bits)
}
