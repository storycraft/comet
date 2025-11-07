pub mod container;
pub mod resolve;

pub use anyrender::Paint;
pub use parley::style::{
    FontFamily, FontFeature, FontSettings, FontStack, FontStyle, FontVariation, FontWeight,
    FontWidth, LineHeight, OverflowWrap, WordBreakStrength,
};
use slotmap::new_key_type;
pub use taffy::{
    Rect, Size,
    style::{BoxSizing, Dimension, LengthPercentage, LengthPercentageAuto, Position},
};

new_key_type! { pub struct LayoutStyleKey; }

/// Style for a specific [`crate::layout::Layout`]
pub trait LayoutStyle: 'static + Sized + Clone {
    type Resolved: Clone;

    fn resolve(&self, parent: &Self::Resolved) -> Self::Resolved;
}

#[derive(Debug, Clone, PartialEq)]
/// Text styles
pub struct TextStyle {
    // Font settings
    pub font_stack: FontStack<'static>,
    pub font_size: f32,
    pub font_width: FontWidth,
    pub font_style: FontStyle,
    pub font_weight: FontWeight,
    pub font_variations: FontSettings<'static, FontVariation>,
    pub font_features: FontSettings<'static, FontFeature>,

    // Locale
    pub locale: Option<&'static str>,

    // Underline
    pub underline_offset: f32,
    pub underline_size: f32,

    // Strikethrough
    pub strikethrough_offset: f32,
    pub strikethrough_size: f32,

    // Line settings
    pub line_height: LineHeight,
    pub overflow_wrap: OverflowWrap,
    pub word_break: WordBreakStrength,

    // Text spacing
    pub word_spacing: f32,
    pub letter_spacing: f32,
}
