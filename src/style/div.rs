pub use anyrender::Paint;
pub use kurbo::{Cap, Dashes, Join};
use taffy::Overflow;
pub use taffy::{
    BoxSizing, Dimension, LengthPercentage, LengthPercentageAuto, Point, Position, Rect, Size,
};

use crate::{
    layout::{DisplayInner, DisplayOuter},
    style::{StyleRect, StyleUnit, define_style_props},
};

// TODO:: change names
define_style_props!(
    // Display
    pub DisplayOuter1: DisplayOuter,
    pub DisplayInner1: DisplayInner,

    // Position
    pub Position1: Position,
    pub Inset: Rect<LengthPercentageAuto>,

    // Size and modes
    pub BoxSizing1: BoxSizing,
    pub Size1: Size<Dimension>,
    pub MinSize: Size<Dimension>,
    pub MaxSize: Size<Dimension>,
    pub AspectRatio: f32,
    pub Overflow1: Point<Overflow>,

    // Margin, padding
    pub Margin: Rect<LengthPercentageAuto>,
    pub Padding: Rect<LengthPercentage>,

    // Box draw styles
    pub Fill: Paint,
    pub BorderFill: Paint,

    // border styles.
    pub BorderWidth: Rect<LengthPercentage>,
    pub BorderRadius: StyleRect,
    pub BorderJoin: Join,
    pub BorderMiterLimit: StyleUnit,
    pub BorderStartCap: Cap,
    pub BorderEndCap: Cap,
    pub BorderDashPattern: Dashes,
    pub BorderDashOffset: StyleUnit,
);
