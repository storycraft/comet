pub mod bx;
pub mod geometry;
pub mod kurbo;
pub mod parley;
pub mod taffy;

use crate::style::geometry::{Size, UnitResolveContext};

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutConstraint {
    /// Viewport and font sizes resolving relative units.
    pub units: UnitResolveContext,

    pub parent_size: Size<f64>,
    pub available_size: Size<f64>,
}

impl LayoutConstraint {
    pub const fn new_root(root_size: Size<f64>, font_size: f64) -> Self {
        Self {
            units: UnitResolveContext {
                root_font_size: font_size,
                font_size,
                viewport: root_size,
            },

            parent_size: root_size,
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
            units: UnitResolveContext {
                root_font_size: self.units.root_font_size,
                font_size,
                viewport: self.units.viewport,
            },

            parent_size: percentage_resolution_size,
            available_size,
        }
    }
}
