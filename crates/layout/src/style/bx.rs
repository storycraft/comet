use comet_tree::style::{StyleProp, define_style_props};
use kurbo;
use parley;
use taffy;

use crate::{
    container::ContainerLayout,
    style::geometry::{self, AutoValue, Point, Rect, StyleUnit},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayOuter {
    #[default]
    /// Element generates a block layout box.
    Block,
    /// Element is part of inline content.
    Inline,
}
impl StyleProp for DisplayOuter {}

#[derive(Default)]
pub enum DisplayInner {
    #[default]
    /// Generate layout boxes and display children using Flow layout.
    Flow,
    /// Lay out independent child boxes using a custom algorithm.
    Container(Box<dyn ContainerLayout>),
    /// Establish a new flow context and layout children inside.
    FlowRoot,
    /// Display a content inside. Children will not be laid out.
    Content,
}
impl StyleProp for DisplayInner {}

define_style_props!(
    // Position
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Position: taffy::Position = taffy::Position::Relative;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub Inset: Rect<AutoValue<StyleUnit>> = Rect::default();

    // Size and modes
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub Size: geometry::Size<AutoValue<StyleUnit>> = geometry::Size::default();
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub MinSize: geometry::Size<AutoValue<StyleUnit>> = geometry::Size::default();
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub MaxSize: geometry::Size<AutoValue<StyleUnit>> = geometry::Size::default();
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub AspectRatio: f32;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Overflow: Point<taffy::Overflow> = Point::default();
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub BoxSizing: taffy::BoxSizing = taffy::BoxSizing::BorderBox;

    // Margin; padding
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub Margin: Rect<AutoValue<StyleUnit>> = Rect::new(
        Point::new(AutoValue::value_default(), AutoValue::value_default()),
        Point::new(AutoValue::value_default(), AutoValue::value_default()),
    );
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub Padding: Rect<StyleUnit> = Rect::default();

    // border styles.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub Border: Rect<StyleUnit> = Rect::default();
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub BorderJoin: kurbo::Join = kurbo::Join::Bevel;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub BorderMiterLimit: StyleUnit = StyleUnit::ZERO;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub BorderStartCap: kurbo::Cap = kurbo::Cap::Butt;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub BorderEndCap: kurbo::Cap = kurbo::Cap::Butt;
    #[derive(Debug, Clone, PartialEq)]
    pub BorderDashPattern: kurbo::Dashes = kurbo::Dashes::default();
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub BorderDashOffset: StyleUnit = StyleUnit::ZERO;

    // Font settings
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub Font: parley::FontFamily<'static>;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub FontSize: StyleUnit;
    #[derive(Debug, Clone, PartialEq)]
    pub FontVariations: parley::FontVariations<'static>;
    #[derive(Debug, Clone, PartialEq)]
    pub FontFeatures: parley::FontFeatures<'static>;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub FontStyle: parley::FontStyle;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub FontWeight: parley::FontWeight;

    // Locale
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Locale: parley::Language;

    // Underline
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub UnderlineOffset: StyleUnit;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub UnderlineSize: StyleUnit;

    // Strikethrough
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub StrikethroughOffset: StyleUnit;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub StrikethroughSize: StyleUnit;

    // Line settings
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub LineHeight: StyleUnit;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub WordBreak: parley::WordBreak;

    // Text spacing
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub WordSpacing: StyleUnit;
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub LetterSpacing: StyleUnit;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub TextAlign: parley::Alignment;
);
