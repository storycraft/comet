use std::sync::Arc;

use comet_tree::{node::NodeKey, ui::Ui};

use super::TextStyle;
use crate::style::{
    bx,
    geometry::{StyleUnit, UnitResolveContext},
};

/// Resolve a text length. Percentages and `em` use `cx.font_size`.
fn resolve_length(value: StyleUnit, cx: &UnitResolveContext) -> f32 {
    value.resolve2(cx) as f32
}

/// Overlay explicit properties on the parent's computed style.
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
        percentage_basis: style.font_size as f64,
        font_size: style.font_size as f64,
        ..cx.clone()
    };

    macro_rules! set {
        ($field:ident, $value:expr) => {
            let value = $value;
            if style.$field != value {
                Arc::make_mut(&mut style).$field = value;
            }
        };
    }

    if let Some(value) = ui.prop::<bx::FontSize>(node) {
        set!(font_size, resolve_length(value.0, &cx));
    }

    cx.font_size = style.font_size as f64;
    cx.percentage_basis = cx.font_size;

    macro_rules! inherit {
        ($prop:ident, $field:ident) => {
            if let Some(value) = ui.prop::<bx::$prop>(node) {
                if style.$field != value.0 {
                    Arc::make_mut(&mut style).$field = value.0.clone();
                }
            }
        };
        ($prop:ident, $field:ident, length) => {
            if let Some(value) = ui.prop::<bx::$prop>(node) {
                set!($field, resolve_length(value.0, &cx));
            }
        };
        ($prop:ident, $field:ident, optional) => {
            if let Some(value) = ui.prop::<bx::$prop>(node) {
                set!($field, Some(resolve_length(value.0, &cx)));
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
        set!(locale, Some(value.0));
    }
    if let Some(value) = ui.prop::<bx::LineHeight>(node) {
        set!(
            line_height,
            parley::LineHeight::Absolute(resolve_length(value.0, &cx))
        );
    }

    style
}
