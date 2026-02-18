use comet_div::{define_style_props, style::PropLevel};
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
    pub Font(PropLevel::FullLayout): FontStack<'static> = FontStack::Single(FontFamily::Generic(GenericFamily::SansSerif)),
    pub FontSize(PropLevel::FullLayout): StyleUnit = StyleUnit::Em(1.0),
    pub FontVariations(PropLevel::FullLayout): FontSettings<'static, FontVariation> = FontSettings::List(Cow::Borrowed(&[])),
    pub FontFeatures(PropLevel::FullLayout): FontSettings<'static, FontFeature> = FontSettings::List(Cow::Borrowed(&[])),
    pub FontStyle1(PropLevel::FullLayout): FontStyle = FontStyle::Normal,
    pub FontWeight1(PropLevel::FullLayout): FontWeight = FontWeight::NORMAL,

    // Locale
    pub Locale(PropLevel::FullLayout): &'static str = "en",

    // Underline
    pub UnderlineOffset(PropLevel::FullLayout): StyleUnit = StyleUnit::ZERO,
    pub UnderlineSize(PropLevel::FullLayout): StyleUnit = StyleUnit::ZERO,

    // Strikethrough
    pub StrikethroughOffset(PropLevel::FullLayout): StyleUnit = StyleUnit::ZERO,
    pub StrikethroughSize(PropLevel::FullLayout): StyleUnit = StyleUnit::ZERO,

    // Line settings
    pub LineHeight(PropLevel::FullLayout): StyleUnit = StyleUnit::ZERO,
    pub WordBreak(PropLevel::FullLayout): WordBreakStrength = WordBreakStrength::Normal,

    // Text spacing
    pub WordSpacing(PropLevel::FullLayout): StyleUnit = StyleUnit::ZERO,
    pub LetterSpacing(PropLevel::FullLayout): StyleUnit = StyleUnit::ZERO,
);
