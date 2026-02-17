use comet_div::define_style_props;
use parley::style::{
    FontFamily, FontFeature, FontSettings, FontStack, FontVariation, GenericFamily,
    WordBreakStrength,
};
use std::borrow::Cow;

pub use parley::style::{FontStyle, FontWeight};

use crate::style::StyleUnit;

// TODO:: change names
define_style_props!(
    // Font settings
    pub Font: FontStack<'static> = FontStack::Single(FontFamily::Generic(GenericFamily::SansSerif)),
    pub FontSize: StyleUnit = StyleUnit::Em(1.0),
    pub FontVariations: FontSettings<'static, FontVariation> = FontSettings::List(Cow::Borrowed(&[])),
    pub FontFeatures: FontSettings<'static, FontFeature> = FontSettings::List(Cow::Borrowed(&[])),
    pub FontStyle1: FontStyle = FontStyle::Normal,
    pub FontWeight1: FontWeight = FontWeight::NORMAL,

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
    pub WordBreak: WordBreakStrength = WordBreakStrength::Normal,

    // Text spacing
    pub WordSpacing: StyleUnit = StyleUnit::ZERO,
    pub LetterSpacing: StyleUnit = StyleUnit::ZERO,
);
