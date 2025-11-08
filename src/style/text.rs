pub use parley::style::{
    FontFamily, FontFeature, FontSettings, FontStack, FontStyle, FontVariation, FontWeight,
    FontWidth, OverflowWrap, WordBreakStrength,
};

use crate::style::{LayoutStyle, LayoutStyleCx, StyleUnit};
use parley::LineHeight;

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
    pub fn new() -> Self {
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

impl LayoutStyle for TextStyle {
    type Resolved = parley::TextStyle<'static, ()>;

    fn resolve(&self, cx: &LayoutStyleCx, parent: &Self::Resolved) -> Self::Resolved {
        fn resolve_or_default(
            unit: Option<StyleUnit>,
            cx: &LayoutStyleCx,
            parent_value: f32,
        ) -> f32 {
            let Some(unit) = unit else {
                return parent_value;
            };

            unit.resolve(cx) as f32
        }

        let strikethrough_size = resolve_or_default(
            self.strikethrough_size,
            cx,
            parent.strikethrough_size.unwrap(),
        );
        let underline_size =
            resolve_or_default(self.underline_size, cx, parent.underline_size.unwrap());

        parley::TextStyle {
            font_stack: self
                .font_stack
                .as_ref()
                .unwrap_or(&parent.font_stack)
                .clone(),
            font_size: resolve_or_default(self.font_size, cx, parent.font_size),
            font_width: self
                .font_width
                .as_ref()
                .unwrap_or(&parent.font_width)
                .clone(),
            font_style: self.font_style.unwrap_or(parent.font_style),
            font_weight: self.font_weight.unwrap_or(parent.font_weight),
            font_variations: self
                .font_variations
                .as_ref()
                .unwrap_or(&parent.font_variations)
                .clone(),
            font_features: self
                .font_features
                .as_ref()
                .unwrap_or(&parent.font_features)
                .clone(),
            locale: self.locale,
            brush: (),
            has_underline: underline_size != 0.0,
            underline_offset: Some(resolve_or_default(
                self.underline_offset,
                cx,
                parent.underline_offset.unwrap_or_default(),
            )),
            underline_size: Some(underline_size),
            underline_brush: None,
            has_strikethrough: strikethrough_size != 0.0,
            strikethrough_offset: Some(resolve_or_default(
                self.strikethrough_offset,
                cx,
                parent.strikethrough_offset.unwrap_or_default(),
            )),
            strikethrough_size: Some(strikethrough_size),
            strikethrough_brush: None,
            line_height: self
                .line_height
                .map(|v| LineHeight::Absolute(v.resolve(cx) as f32))
                .unwrap_or(parent.line_height),
            word_spacing: resolve_or_default(self.word_spacing, cx, parent.word_spacing),
            letter_spacing: resolve_or_default(self.letter_spacing, cx, parent.letter_spacing),
            word_break: self.word_break.unwrap_or(parent.word_break),
            overflow_wrap: self.overflow_wrap.unwrap_or(parent.overflow_wrap),
        }
    }
}
