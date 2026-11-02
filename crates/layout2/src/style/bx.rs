use std::borrow::Cow;

use comet_tree::style::{StyleProp, define_style_props};
use kurbo;
use parley;
use taffy;

use crate::style::geometry::{self, AutoValue, Point, Rect, StyleUnit};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayOuter {
    #[default]
    /// Element generates a block layout box.
    Block,
    /// Element is part of inline content.
    Inline,
}
impl StyleProp for DisplayOuter {}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum DisplayInner {
    #[default]
    /// Generate layout boxes and display children using Flow layout.
    Flow,
    /// Establish a new flow context and layout children inside.
    FlowRoot,
    /// Display a content inside. Children will not be laid out.
    Content,
}
impl StyleProp for DisplayInner {}

define_style_props!(
    // Position
    pub Position: taffy::Position = taffy::Position::Relative;
    pub Inset: Rect<AutoValue<StyleUnit>>;

    // Size and modes
    pub Size: geometry::Size<AutoValue<StyleUnit>>;
    pub MinSize: geometry::Size<AutoValue<StyleUnit>>;
    pub MaxSize: geometry::Size<AutoValue<StyleUnit>>;
    pub AspectRatio: f32;
    pub Overflow: Point<taffy::Overflow>;
    pub BoxSizing: taffy::BoxSizing = taffy::BoxSizing::BorderBox;

    // Margin; padding
    pub Margin: Rect<AutoValue<StyleUnit>>;
    pub Padding: Rect<StyleUnit>;

    // border styles.
    pub BorderWidth: StyleUnit;
    pub BorderRadius: Rect<StyleUnit>;
    pub BorderJoin: kurbo::Join = kurbo::Join::Bevel;
    pub BorderMiterLimit: StyleUnit;
    pub BorderStartCap: kurbo::Cap = kurbo::Cap::Butt;
    pub BorderEndCap: kurbo::Cap = kurbo::Cap::Butt;
    pub BorderDashPattern: kurbo::Dashes;
    pub BorderDashOffset: StyleUnit;

    // Font settings
    pub Font: parley::FontFamily<'static> = parley::FontFamily::Single(
        parley::FontFamilyName::Generic(parley::GenericFamily::SansSerif)
    );
    pub FontSize: StyleUnit;
    pub FontVariations: parley::FontVariations<'static> = parley::FontVariations::List(Cow::Borrowed(&[]));
    pub FontFeatures: parley::FontFeatures<'static> = parley::FontFeatures::List(Cow::Borrowed(&[]));
    pub FontStyle: parley::FontStyle;
    pub FontWeight: parley::FontWeight;

    // Locale
    pub Locale: parley::Language = parley::Language::UND;

    // Underline
    pub UnderlineOffset: StyleUnit = StyleUnit::ZERO;
    pub UnderlineSize: StyleUnit = StyleUnit::ZERO;

    // Strikethrough
    pub StrikethroughOffset: StyleUnit = StyleUnit::ZERO;
    pub StrikethroughSize: StyleUnit = StyleUnit::ZERO;

    // Line settings
    pub LineHeight: StyleUnit = StyleUnit::ZERO;
    pub WordBreak: parley::style::WordBreak = parley::style::WordBreak::Normal;

    // Text spacing
    pub WordSpacing: StyleUnit = StyleUnit::ZERO;
    pub LetterSpacing: StyleUnit = StyleUnit::ZERO;
);
