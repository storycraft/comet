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
    pub Position: taffy::Position = taffy::Position::Relative;
    pub Inset: Rect<AutoValue<StyleUnit>> = Rect::default();

    // Size and modes
    pub Size: geometry::Size<AutoValue<StyleUnit>> = geometry::Size::default();
    pub MinSize: geometry::Size<AutoValue<StyleUnit>> = geometry::Size::default();
    pub MaxSize: geometry::Size<AutoValue<StyleUnit>> = geometry::Size::default();
    pub AspectRatio: f32;
    pub Overflow: Point<taffy::Overflow> = Point::default();
    pub BoxSizing: taffy::BoxSizing = taffy::BoxSizing::BorderBox;

    // Margin; padding
    pub Margin: Rect<AutoValue<StyleUnit>> = Rect::new(
        Point::new(AutoValue::value_default(), AutoValue::value_default()),
        Point::new(AutoValue::value_default(), AutoValue::value_default()),
    );
    pub Padding: Rect<StyleUnit> = Rect::default();

    // border styles.
    pub Border: Rect<StyleUnit> = Rect::default();
    pub BorderJoin: kurbo::Join = kurbo::Join::Bevel;
    pub BorderMiterLimit: StyleUnit = StyleUnit::ZERO;
    pub BorderStartCap: kurbo::Cap = kurbo::Cap::Butt;
    pub BorderEndCap: kurbo::Cap = kurbo::Cap::Butt;
    pub BorderDashPattern: kurbo::Dashes = kurbo::Dashes::default();
    pub BorderDashOffset: StyleUnit = StyleUnit::ZERO;

    // Font settings
    pub Font: parley::FontFamily<'static>;
    pub FontSize: StyleUnit;
    pub FontVariations: parley::FontVariations<'static>;
    pub FontFeatures: parley::FontFeatures<'static>;
    pub FontStyle: parley::FontStyle;
    pub FontWeight: parley::FontWeight;

    // Locale
    pub Locale: parley::Language;

    // Underline
    pub UnderlineOffset: StyleUnit;
    pub UnderlineSize: StyleUnit;

    // Strikethrough
    pub StrikethroughOffset: StyleUnit;
    pub StrikethroughSize: StyleUnit;

    // Line settings
    pub LineHeight: StyleUnit;
    pub WordBreak: parley::style::WordBreak;

    // Text spacing
    pub WordSpacing: StyleUnit;
    pub LetterSpacing: StyleUnit;
);
