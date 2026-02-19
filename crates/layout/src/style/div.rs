use crate::{
    layout_box::ContainerLayout,
    style::{StyleRect, StyleUnit},
};
use comet_div::{define_style_props, style::PropHint, style_prop};
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
style_prop!(DisplayOuter = PropHint::Layout);

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
style_prop!(DisplayInner = PropHint::Layout);

// TODO:: change names
define_style_props!(
    // Position
    pub Position1(PropHint::FullLayout): taffy::Position = taffy::Position::Relative,
    pub Inset(PropHint::FullLayout): Rect<LengthPercentageAuto> = Rect::auto(),

    // Size and modes
    pub Size1(PropHint::FullLayout): Size<Dimension> = Size::auto(),
    pub MinSize(PropHint::FullLayout): Size<Dimension> = Size::auto(),
    pub MaxSize(PropHint::FullLayout): Size<Dimension> = Size::auto(),
    pub AspectRatio(PropHint::FullLayout): f32,
    pub Overflow1(PropHint::FullLayout): Point<Overflow> = Point { x: Overflow::Visible, y: Overflow::Visible },
    pub BoxSizing1(PropHint::FullLayout): BoxSizing = BoxSizing::BorderBox,

    // Margin, padding
    pub Margin(PropHint::FullLayout): Rect<LengthPercentageAuto> = Rect::zero(),
    pub Padding(PropHint::FullLayout): Rect<LengthPercentage> = Rect::zero(),

    // border styles.
    pub BorderWidth(PropHint::FullLayout): LengthPercentage = LengthPercentage::length(0.0),
    pub BorderRadius(PropHint::FullLayout): StyleRect,
    pub BorderJoin(PropHint::FullLayout): Join = Join::Bevel,
    pub BorderMiterLimit(PropHint::FullLayout): StyleUnit,
    pub BorderStartCap(PropHint::FullLayout): Cap = Cap::Butt,
    pub BorderEndCap(PropHint::FullLayout): Cap = Cap::Butt,
    pub BorderDashPattern(PropHint::FullLayout): Dashes,
    pub BorderDashOffset(PropHint::FullLayout): StyleUnit,
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
