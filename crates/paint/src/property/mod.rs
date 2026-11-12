pub mod tree;

use anyrender::Paint;
use kurbo::{Affine, Point, Rect};
use slotmap::new_key_type;

pub struct PaintProperties {
    pub transform: Affine,
    pub text_color: Paint,
    pub clip: Option<Rect>,
}

#[derive(Clone, Copy)]
pub struct FragmentData {
    pub property: PropertyKey,

    /// Absolute position of fragment box.
    pub position: Point,
}

new_key_type! {
    pub struct PropertyKey;
}
