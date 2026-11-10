//! Inherited text styles of input items, resolved for Parley inline layout.

use std::sync::Arc;

use comet_tree::{node::NodeKey, ui::Ui};

use super::LayoutInputKey;
use crate::style::{
    bx,
    geometry::{StyleUnit, UnitResolveContext},
    parley::Alignment,
};

/// Parley style of a text span. Its brush is set while shaping to the item
/// owning the span, so glyph runs map back to their item.
pub type SpanStyle = ::parley::TextStyle<'static, 'static, Option<LayoutInputKey>>;

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
fn resolve_length(value: StyleUnit, cx: &UnitResolveContext) -> f32 {
    value.resolve2(cx) as f32
}

/// Overlay explicit properties on the parent's computed style, including the
/// span style and the text alignment.
///
/// `cx` supplies the root font size and viewport; its `font_size` and
/// `percentage_basis` are overwritten. `font-size` resolves against the parent's
/// font size, while every other length resolves against the resolved font size.
/// The parent allocation is shared when no property changes the style.
pub fn resolve_text_style(
    ui: &Ui,
    node: NodeKey,
    cx: &UnitResolveContext,
    parent: &Arc<TextStyle>,
) -> Arc<TextStyle> {
    let mut style = parent.clone();
    let mut cx = UnitResolveContext {
        percentage_basis: style.span.font_size as f64,
        font_size: style.span.font_size as f64,
        ..cx.clone()
    };

    macro_rules! set {
        ($($field:ident).+, $value:expr) => {
            let value = $value;
            if style.$($field).+ != value {
                Arc::make_mut(&mut style).$($field).+ = value;
            }
        };
    }

    if let Some(value) = ui.prop::<bx::FontSize>(node) {
        set!(span.font_size, resolve_length(value.0, &cx));
    }

    cx.font_size = style.span.font_size as f64;
    cx.percentage_basis = cx.font_size;

    macro_rules! inherit {
        ($prop:ident, $field:ident) => {
            if let Some(value) = ui.prop::<bx::$prop>(node) {
                if style.span.$field != value.0 {
                    Arc::make_mut(&mut style).span.$field = value.0.clone();
                }
            }
        };
        ($prop:ident, $field:ident, length) => {
            if let Some(value) = ui.prop::<bx::$prop>(node) {
                set!(span.$field, resolve_length(value.0, &cx));
            }
        };
        ($prop:ident, $field:ident, optional) => {
            if let Some(value) = ui.prop::<bx::$prop>(node) {
                set!(span.$field, Some(resolve_length(value.0, &cx)));
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

    if let Some(value) = ui.prop::<bx::Locale>(node) {
        set!(span.locale, Some(value.0));
    }
    if let Some(value) = ui.prop::<bx::LineHeight>(node) {
        set!(
            span.line_height,
            parley::LineHeight::Absolute(resolve_length(value.0, &cx))
        );
    }
    if let Some(value) = ui.prop::<bx::TextAlign>(node) {
        set!(align, value.0);
    }

    style
}
