use crate::{
    layout::ContainerLayout,
    style::{StyleRect, StyleUnit, define_style_props},
};
use anyrender::Paint;
use kurbo::{Cap, Dashes, Join};
use taffy::{Dimension, LengthPercentage, LengthPercentageAuto, Overflow, Point, Rect, Size};

pub use taffy::{BoxSizing, Position};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayOuter {
    #[default]
    /// Element generates a block layout box.
    Block,
    /// Element is part of inline content.
    Inline,
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

// TODO:: change names
define_style_props!(
    // Position
    pub Inset: Rect<LengthPercentageAuto> = Rect::auto(),

    // Size and modes
    pub Size1: Size<Dimension> = Size::auto(),
    pub MinSize: Size<Dimension> = Size::auto(),
    pub MaxSize: Size<Dimension> = Size::auto(),
    pub AspectRatio: f32,
    pub Overflow1: Point<Overflow> = Point { x: Overflow::Visible, y: Overflow::Visible },

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
