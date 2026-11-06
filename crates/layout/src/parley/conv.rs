use std::sync::Arc;

use comet_tree::{node::NodeKey, ui::Ui};

use super::TextStyle;
use crate::style::{LayoutConstraint, bx, geometry::StyleUnit};

/// Text percentages use the relevant font size, never the containing box size.
pub fn length(value: StyleUnit, cx: &LayoutConstraint) -> f32 {
    value.resolve(cx, cx.font_size) as f32
}

/// Overlay explicit properties on the parent's computed style.
pub fn text_style(
    ui: &Ui,
    node: NodeKey,
    cx: &LayoutConstraint,
    parent: &Arc<TextStyle>,
) -> Arc<TextStyle> {
    let mut style = parent.clone();
    let mut cx = cx.clone();
    cx.font_size = style.font_size as f64;

    macro_rules! set {
        ($field:ident, $value:expr) => {
            let value = $value;
            if style.$field != value {
                Arc::make_mut(&mut style).$field = value;
            }
        };
    }

    if let Some(value) = ui.prop::<bx::FontSize>(node) {
        set!(font_size, length(value.0, &cx));
    }

    cx.font_size = style.font_size as f64;
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
                set!($field, length(value.0, &cx));
            }
        };
        ($prop:ident, $field:ident, optional) => {
            if let Some(value) = ui.prop::<bx::$prop>(node) {
                set!($field, Some(length(value.0, &cx)));
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
            parley::LineHeight::Absolute(length(value.0, &cx))
        );
    }

    style
}
