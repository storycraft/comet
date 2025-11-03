use slotmap::new_key_type;
use taffy::{
    BoxSizing, Dimension, LengthPercentage, LengthPercentageAuto, Overflow, Point, Position, Rect,
    Size,
};

use crate::layout::{DisplayInner, DisplayOuter};

new_key_type! { pub struct NodeKey; }

#[derive(Debug, PartialEq)]
pub enum Node {
    Div(Div),
    Text(String),
}

#[derive(Debug, PartialEq)]
pub struct Div {
    // Display
    pub display_outer: Option<DisplayOuter>,
    pub display_inner: DisplayInner,

    // Position
    pub position: Position,
    pub inset: Rect<LengthPercentageAuto>,

    // Size and modes
    pub box_sizing: BoxSizing,
    pub size: Size<Dimension>,
    pub min_size: Size<Dimension>,
    pub max_size: Size<Dimension>,
    pub aspect_ratio: Option<f32>,
    pub overflow: Point<Overflow>,

    // Margin, padding, border
    pub margin: Rect<LengthPercentageAuto>,
    pub padding: Rect<LengthPercentage>,
    pub border: Rect<LengthPercentage>,
}

impl Div {
    pub const fn new() -> Self {
        Self {
            display_outer: Some(DisplayOuter::Block),
            display_inner: DisplayInner::Flow,

            position: Position::Relative,
            inset: Rect::zero(),

            box_sizing: BoxSizing::BorderBox,
            size: Size::auto(),
            min_size: Size::auto(),
            max_size: Size::auto(),
            aspect_ratio: None,
            overflow: Point {
                x: Overflow::Visible,
                y: Overflow::Visible,
            },

            margin: Rect::zero(),
            padding: Rect::zero(),
            border: Rect::zero(),
        }
    }
}

impl Default for Div {
    fn default() -> Self {
        Self::new()
    }
}
