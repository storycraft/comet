pub mod bx;
pub mod geometry;
pub mod kurbo;
pub mod parley;
pub mod taffy;

use crate::style::geometry::Size;

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutConstraint {
    pub root_size: Size<f64>,
    pub root_font_size: f64,

    pub parent_size: Size<f64>,
    pub font_size: f64,

    pub available_size: Size<f64>,
}

impl LayoutConstraint {
    pub const fn new_root(root_size: Size<f64>, font_size: f64) -> Self {
        Self {
            root_size,
            root_font_size: font_size,

            parent_size: root_size,
            font_size,

            available_size: root_size,
        }
    }

    pub const fn child(
        &self,
        available_size: Size<f64>,
        percentage_resolution_size: Size<f64>,
        font_size: f64,
    ) -> Self {
        Self {
            root_size: self.root_size,
            root_font_size: self.root_font_size,

            parent_size: percentage_resolution_size,
            font_size,

            available_size,
        }
    }
}
