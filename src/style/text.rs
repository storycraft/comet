pub use parley::style::{
    FontFamily, FontFeature, FontSettings, FontStack, FontStyle, FontVariation, FontWeight,
    FontWidth, OverflowWrap, WordBreakStrength,
};

use crate::style::StyleUnit;

#[derive(Debug, Clone, PartialEq)]
/// Text styles
pub struct TextStyle {
    // Font settings
    pub font_stack: Option<FontStack<'static>>,
    pub font_size: Option<StyleUnit>,
    pub font_width: Option<FontWidth>,
    pub font_style: Option<FontStyle>,
    pub font_weight: Option<FontWeight>,
    pub font_variations: Option<FontSettings<'static, FontVariation>>,
    pub font_features: Option<FontSettings<'static, FontFeature>>,

    // Locale
    pub locale: Option<&'static str>,

    // Underline
    pub underline_offset: Option<StyleUnit>,
    pub underline_size: Option<StyleUnit>,

    // Strikethrough
    pub strikethrough_offset: Option<StyleUnit>,
    pub strikethrough_size: Option<StyleUnit>,

    // Line settings
    pub line_height: Option<StyleUnit>,
    pub overflow_wrap: Option<OverflowWrap>,
    pub word_break: Option<WordBreakStrength>,

    // Text spacing
    pub word_spacing: Option<StyleUnit>,
    pub letter_spacing: Option<StyleUnit>,
}

impl TextStyle {
    #[inline]
    pub const fn new() -> Self {
        Self {
            font_stack: None,
            font_size: None,
            font_width: None,
            font_style: None,
            font_weight: None,
            font_variations: None,
            font_features: None,
            locale: None,
            underline_offset: None,
            underline_size: None,
            strikethrough_offset: None,
            strikethrough_size: None,
            line_height: None,
            overflow_wrap: None,
            word_break: None,
            word_spacing: None,
            letter_spacing: None,
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::new()
    }
}
