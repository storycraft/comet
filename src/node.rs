use anyrender::Paint;
use kurbo::{Affine, RoundedRectRadii, Stroke};
use slotmap::new_key_type;
use taffy::{
    BoxSizing, Dimension, LengthPercentage, LengthPercentageAuto, Overflow, Point, Position, Rect,
    Size,
};

use crate::layout::{DisplayInner, DisplayOuter};

new_key_type! { pub struct NodeKey; }

#[derive(Debug)]
pub enum Node {
    Div(Div),
    Text(String),
}

#[derive(Debug)]
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

    // Box draw styles
    // TODO:: move to a separate style struct
    pub transform: Affine,
    pub fill: Option<Paint>,
    pub border_radius: RoundedRectRadii,
    pub stroke: Option<(Stroke, Paint)>,
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

            transform: Affine::IDENTITY,
            fill: None,
            border_radius: RoundedRectRadii::new(0.0, 0.0, 0.0, 0.0),
            stroke: None,
        }
    }
}

impl Default for Div {
    fn default() -> Self {
        Self::new()
    }
}
