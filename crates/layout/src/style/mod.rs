pub mod bx;

use comet_geometry::{Size, UnitResolveContext};

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutConstraint {
    /// Viewport and font sizes resolving relative units.
    pub units: UnitResolveContext,

    pub parent_size: Size<f64>,
    pub available_size: Size<f64>,

    /// Physical pixels per logical unit of the display. Layout works in
    /// logical units: a physical size is the logical size times this, and a
    /// logical size the physical size divided by it. Contents use it to
    /// measure their physical sizes, such as the pixels of an image, in
    /// logical units.
    pub scale_factor: f64,

    /// Whether the viewport or the scale factor differs from the previous
    /// layout. Boxes depending on the viewport then resolve their styles and
    /// are laid out again, and so are boxes displaying a content.
    pub viewport_changed: bool,
}

impl LayoutConstraint {
    /// The constraint of the root box: laid out in `root_size`, the
    /// viewport, with the root `font_size`, on a display of `scale_factor`
    /// physical pixels per logical unit.
    pub const fn new_root(root_size: Size<f64>, font_size: f64, scale_factor: f64) -> Self {
        Self {
            units: UnitResolveContext {
                root_font_size: font_size,
                font_size,
                viewport: root_size,
            },

            parent_size: root_size,
            available_size: root_size,
            scale_factor,
            viewport_changed: false,
        }
    }

    /// Set [`Self::viewport_changed`].
    pub const fn with_viewport_changed(mut self, viewport_changed: bool) -> Self {
        self.viewport_changed = viewport_changed;
        self
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
            scale_factor: self.scale_factor,
            viewport_changed: self.viewport_changed,
        }
    }
}
