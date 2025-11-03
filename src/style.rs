pub mod tree;

pub use parley::style::{
    FontFamily, FontFeature, FontSettings, FontStack, FontStyle, FontVariation, FontWeight,
    FontWidth, LineHeight, OverflowWrap, WordBreakStrength,
};

pub use taffy::{
    Rect, Size,
    style::{BoxSizing, Dimension, LengthPercentage, LengthPercentageAuto, Position},
};

use crate::layout::{DisplayInner, DisplayOuter};

pub trait GenericStyle: 'static + Sized + Clone {
    
}

#[derive(Debug, Clone, PartialEq)]
/// Style properties for flow layout
pub struct Style<'a> {
    // Display
    pub display_outer: Option<DisplayOuter>,
    pub display_inner: Option<DisplayInner>,

    // Position
    pub position: Position,

    // Size and modes
    pub box_sizing: BoxSizing,
    pub size: Size<Dimension>,
    pub min_size: Size<Dimension>,
    pub max_size: Size<Dimension>,

    // Margin, padding, border
    pub margin: Rect<LengthPercentageAuto>,
    pub padding: Rect<LengthPercentage>,
    pub border: Rect<LengthPercentage>,

    // Font settings
    pub font_stack: FontStack<'a>,
    pub font_size: f32,
    pub font_width: FontWidth,
    pub font_style: FontStyle,
    pub font_weight: FontWeight,
    pub font_variations: FontSettings<'a, FontVariation>,
    pub font_features: FontSettings<'a, FontFeature>,

    // Locale
    pub locale: Option<&'a str>,

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
