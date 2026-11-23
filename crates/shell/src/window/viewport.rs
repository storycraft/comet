//! Size of the window's surface.

use dpi::PhysicalSize;
use kurbo::{Rect, Size, Vec2};

use crate::event::ScrollDelta;

/// Distance scrolled by one line of a mouse wheel, in logical units.
const WHEEL_LINE: f64 = 40.0;

/// The surface of a window, in physical pixels, and the scale factor
/// mapping it to the logical units of the layout.
pub(super) struct Viewport {
    pub physical_size: PhysicalSize<u32>,
    pub scale_factor: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            physical_size: PhysicalSize::new(0, 0),
            scale_factor: 1.0,
        }
    }
}

impl Viewport {
    /// Size of the viewport, in logical units: the surface divided by the
    /// scale factor.
    pub fn size(&self) -> Size {
        Size::new(
            f64::from(self.physical_size.width) / self.scale_factor,
            f64::from(self.physical_size.height) / self.scale_factor,
        )
    }

    /// The surface, in physical pixels.
    pub fn physical_rect(&self) -> Rect {
        Rect::new(
            0.0,
            0.0,
            f64::from(self.physical_size.width),
            f64::from(self.physical_size.height),
        )
    }

    /// Whether the surface covers no pixel, such as while minimized.
    pub fn is_empty(&self) -> bool {
        self.physical_size.width == 0 || self.physical_size.height == 0
    }

    /// A wheel or touchpad `delta` in logical units, a page being the
    /// viewport.
    pub fn scroll_delta(&self, delta: ScrollDelta) -> Vec2 {
        match delta {
            ScrollDelta::LineDelta(x, y) => {
                Vec2::new(f64::from(x) * WHEEL_LINE, f64::from(y) * WHEEL_LINE)
            }
            ScrollDelta::PageDelta(x, y) => {
                let size = self.size();
                Vec2::new(f64::from(x) * size.width, f64::from(y) * size.height)
            }
            ScrollDelta::PixelDelta(position) => {
                let position = position.to_logical::<f64>(self.scale_factor);
                Vec2::new(position.x, position.y)
            }
        }
    }
}
