use comet_div::{define_style_props, style::PropHint};
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
    pub Font(PropHint::FullLayout): FontStack<'static> = FontStack::Single(FontFamily::Generic(GenericFamily::SansSerif)),
    pub FontSize(PropHint::FullLayout): StyleUnit = StyleUnit::Em(1.0),
    pub FontVariations(PropHint::FullLayout): FontSettings<'static, FontVariation> = FontSettings::List(Cow::Borrowed(&[])),
    pub FontFeatures(PropHint::FullLayout): FontSettings<'static, FontFeature> = FontSettings::List(Cow::Borrowed(&[])),
    pub FontStyle1(PropHint::FullLayout): FontStyle = FontStyle::Normal,
    pub FontWeight1(PropHint::FullLayout): FontWeight = FontWeight::NORMAL,

    // Locale
    pub Locale(PropHint::FullLayout): &'static str = "en",

    // Underline
    pub UnderlineOffset(PropHint::FullLayout): StyleUnit = StyleUnit::ZERO,
    pub UnderlineSize(PropHint::FullLayout): StyleUnit = StyleUnit::ZERO,

    // Strikethrough
    pub StrikethroughOffset(PropHint::FullLayout): StyleUnit = StyleUnit::ZERO,
    pub StrikethroughSize(PropHint::FullLayout): StyleUnit = StyleUnit::ZERO,

    // Line settings
    pub LineHeight(PropHint::FullLayout): StyleUnit = StyleUnit::ZERO,
    pub WordBreak(PropHint::FullLayout): WordBreakStrength = WordBreakStrength::Normal,

    // Text spacing
    pub WordSpacing(PropHint::FullLayout): StyleUnit = StyleUnit::ZERO,
    pub LetterSpacing(PropHint::FullLayout): StyleUnit = StyleUnit::ZERO,
);
