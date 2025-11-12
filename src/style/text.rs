pub use parley::style::{
    FontFamily, FontFeature, FontSettings, FontStack, FontStyle, FontVariation, FontWeight,
    FontWidth, GenericFamily, OverflowWrap, WordBreakStrength,
};

use anyrender::Paint;
use peniko::Color;
use std::borrow::Cow;

use crate::style::{StyleUnit, define_style_props};

// TODO:: change names
define_style_props!(
    // Font settings
    pub FontStack1: FontStack<'static> = FontStack::Single(FontFamily::Generic(GenericFamily::SansSerif)),
    pub FontSize: StyleUnit = StyleUnit::Em(1.0),
    pub FontWidth1: FontWidth = FontWidth::NORMAL,
    pub FontStyle1: FontStyle = FontStyle::Normal,
    pub FontWeight1: FontWeight = FontWeight::NORMAL,
    pub FontVariations: FontSettings<'static, FontVariation> = FontSettings::List(Cow::Borrowed(&[])),
    pub FontFeatures: FontSettings<'static, FontFeature> = FontSettings::List(Cow::Borrowed(&[])),

    // Locale
    pub Locale: &'static str = "en",

    // Underline
    pub UnderlineOffset: StyleUnit = StyleUnit::ZERO,
    pub UnderlineSize: StyleUnit = StyleUnit::ZERO,

    // Strikethrough
    pub StrikethroughOffset: StyleUnit = StyleUnit::ZERO,
    pub StrikethroughSize: StyleUnit = StyleUnit::ZERO,

    // Line settings
    pub LineHeight: StyleUnit = StyleUnit::ZERO,
    pub OverflowWrap1: OverflowWrap = OverflowWrap::Normal,
    pub WordBreak: WordBreakStrength = WordBreakStrength::Normal,

    // Text spacing
    pub WordSpacing: StyleUnit = StyleUnit::ZERO,
    pub LetterSpacing: StyleUnit = StyleUnit::ZERO,

    // Paint
    pub TextColor: Paint = Paint::Solid(Color::BLACK),
);
