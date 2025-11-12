use anyrender::Paint;
pub use parley::style::{
    FontFamily, FontFeature, FontSettings, FontStack, FontStyle, FontVariation, FontWeight,
    FontWidth, OverflowWrap, WordBreakStrength,
};

use crate::{style::{define_style_props, StyleUnit}};

// TODO:: change names
define_style_props!(
    // Font settings
    pub FontStack1: FontStack<'static>,
    pub FontSize: StyleUnit,
    pub FontWidth1: FontWidth,
    pub FontStyle1: FontStyle,
    pub FontWeight1: FontWeight,
    pub FontVariations: FontSettings<'static, FontVariation>,
    pub FontFeatures: FontSettings<'static, FontFeature>,

    // Locale
    pub Locale: &'static str,

    // Underline
    pub UnderlineOffset: StyleUnit,
    pub UnderlineSize: StyleUnit,

    // Strikethrough
    pub StrikethroughOffset: StyleUnit,
    pub StrikethroughSize: StyleUnit,

    // Line settings
    pub LineHeight: StyleUnit,
    pub OverflowWrap1: OverflowWrap,
    pub WordBreak: WordBreakStrength,

    // Text spacing
    pub WordSpacing: StyleUnit,
    pub LetterSpacing: StyleUnit,

    // Paint
    pub TextColor: Paint,
);
