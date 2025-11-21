use crate::{
    style::{PropLevel, StyleUnit, define_style_props},
    style_prop,
};
use anyrender::Paint;
use parley::style::{
    FontFamily, FontFeature, FontSettings, FontStack, FontVariation, GenericFamily,
    WordBreakStrength,
};
use peniko::Color;
use std::borrow::Cow;

pub use parley::style::{FontStyle, FontWeight};

style_prop!(FontStyle = PropLevel::Layout);
style_prop!(FontWeight = PropLevel::Layout);

// TODO:: change names
define_style_props!(
    // Font settings
    pub Font: FontStack<'static> = FontStack::Single(FontFamily::Generic(GenericFamily::SansSerif)),
    pub FontSize: StyleUnit = StyleUnit::Em(1.0),
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
    pub WordBreak: WordBreakStrength = WordBreakStrength::Normal,

    // Text spacing
    pub WordSpacing: StyleUnit = StyleUnit::ZERO,
    pub LetterSpacing: StyleUnit = StyleUnit::ZERO,

    // Paint
    pub TextColor: Paint = Paint::Solid(Color::BLACK),
);
