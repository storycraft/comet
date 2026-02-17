use crate::{
    layout::ContainerLayout,
    style::{StyleRect, StyleUnit},
};
use anyrender::Paint;
use comet_div::{
    define_style_props,
    style::{PropLevel, StyleProp},
};
use kurbo::{Cap, Dashes, Join};
use taffy::{
    BoxSizing, Dimension, LengthPercentage, LengthPercentageAuto, Overflow, Point, Rect, Size,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayOuter {
    #[default]
    /// Element generates a block layout box.
    Block,
    /// Element is part of inline content.
    Inline,
}

impl StyleProp for DisplayOuter {
    const LEVEL: PropLevel = PropLevel::Layout;
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum DisplayInner {
    #[default]
    /// Generate layout boxes and display children using Flow layout.
    Flow,
    /// Establish a new flow context and layout children inside.
    FlowRoot,
    /// Layout each children with given layout algorithm.
    Container(ContainerLayout),
    /// Display a content inside. Children will not be laid out.
    Content,
}

impl StyleProp for DisplayInner {
    const LEVEL: PropLevel = PropLevel::Layout;
}

// TODO:: change names
define_style_props!(
    // Position
    pub Position1: taffy::Position = taffy::Position::Relative,
    pub Inset: Rect<LengthPercentageAuto> = Rect::auto(),

    // Size and modes
    pub Size1: Size<Dimension> = Size::auto(),
    pub MinSize: Size<Dimension> = Size::auto(),
    pub MaxSize: Size<Dimension> = Size::auto(),
    pub AspectRatio: f32,
    pub Overflow1: Point<Overflow> = Point { x: Overflow::Visible, y: Overflow::Visible },
    pub BoxSizing1: BoxSizing = BoxSizing::BorderBox,

    // Margin, padding
    pub Margin: Rect<LengthPercentageAuto> = Rect::zero(),
    pub Padding: Rect<LengthPercentage> = Rect::zero(),

    // Box draw styles
    pub Fill: Paint,
    pub BorderFill: Paint,

    // border styles.
    pub BorderWidth: LengthPercentage = LengthPercentage::length(0.0),
    pub BorderRadius: StyleRect,
    pub BorderJoin: Join = Join::Bevel,
    pub BorderMiterLimit: StyleUnit,
    pub BorderStartCap: Cap = Cap::Butt,
    pub BorderEndCap: Cap = Cap::Butt,
    pub BorderDashPattern: Dashes,
    pub BorderDashOffset: StyleUnit,
);

unsafe impl Send for Inset {}
unsafe impl Sync for Inset {}

unsafe impl Send for Size1 {}
unsafe impl Sync for Size1 {}

unsafe impl Send for MinSize {}
unsafe impl Sync for MinSize {}

unsafe impl Send for Margin {}
unsafe impl Sync for Margin {}

unsafe impl Send for Padding {}
unsafe impl Sync for Padding {}

unsafe impl Send for MaxSize {}
unsafe impl Sync for MaxSize {}

unsafe impl Send for BorderWidth {}
unsafe impl Sync for BorderWidth {}
