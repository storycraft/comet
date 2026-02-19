pub mod conv;

use std::borrow::Cow;

use parley::{
    Brush, FontFamily, FontSettings, FontStack, FontStyle, FontWeight, FontWidth, GenericFamily,
    LineHeight, OverflowWrap, TextStyle, TextWrapMode, WordBreakStrength,
};

pub fn default_text_style<'a, B: Brush>(brush: B) -> TextStyle<'a, B> {
    TextStyle {
        font_stack: FontStack::List(Cow::Borrowed(&[
            FontFamily::Generic(GenericFamily::SansSerif),
            FontFamily::Generic(GenericFamily::Emoji),
        ])),
        font_size: 16.0,
        font_width: FontWidth::NORMAL,
        font_style: FontStyle::Normal,
        font_weight: FontWeight::NORMAL,
        // TODO:: add default variations
        font_variations: FontSettings::List(Cow::Borrowed(&[])),
        // TODO:: add default features
        font_features: FontSettings::List(Cow::Borrowed(&[])),
        locale: None,
        brush,
        has_underline: false,
        underline_offset: None,
        underline_size: None,
        underline_brush: None,
        has_strikethrough: false,
        strikethrough_offset: None,
        strikethrough_size: None,
        strikethrough_brush: None,
        line_height: LineHeight::FontSizeRelative(1.0),
        word_spacing: 0.0,
        letter_spacing: 0.0,
        word_break: WordBreakStrength::Normal,
        overflow_wrap: OverflowWrap::Normal,
        text_wrap_mode: TextWrapMode::Wrap,
    }
}
