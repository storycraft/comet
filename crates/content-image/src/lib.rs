//! Image contents for comet: raster images drawn by replaced boxes, see
//! [`comet_content`].

pub mod style;

use anyrender::{AntiAliasing, Paint, PaintScene, Scene};
use kurbo::{Affine, Rect};
use peniko::{Extend, Fill, ImageBrush, ImageData};

use comet_content::{Content, DrawInput, MeasureInput, fit_transform, style::ObjectStyle};

use crate::style::{ImageExtend, ImageRendering};

/// A raster image, sized and placed in its box by the
/// [content styles](comet_content::style) of the box's node, and sampled
/// and extended by its [image styles](style). Its natural size is its pixel
/// size in logical units: one image pixel per physical pixel of the display.
/// It is drawn opaque.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageContent {
    pub image: ImageData,
}

impl ImageContent {
    pub fn new(image: ImageData) -> Self {
        Self { image }
    }

    /// Pixel size of the image.
    fn pixel_size(&self) -> kurbo::Size {
        kurbo::Size::new(f64::from(self.image.width), f64::from(self.image.height))
    }
}

impl Content for ImageContent {
    fn measure(&self, input: &MeasureInput<'_>) -> taffy::Size<f32> {
        ObjectStyle::of(input.props).measure(self.pixel_size(), input)
    }

    fn draw(&self, input: &DrawInput<'_>, scene: &mut Scene) {
        let rect = ObjectStyle::of(input.props).place(self.pixel_size(), input);
        let extend: ImageExtend = input.props.get_or_default();
        let brush = ImageBrush {
            image: &self.image,
            sampler: style::sampler(input.props.get_or_default::<ImageRendering>(), extend),
        };

        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Paint::Image(brush),
            Some(
                Affine::translate(rect.origin().to_vec2())
                    * fit_transform(self.pixel_size(), rect.size()),
            ),
            &extended(rect, input.size, extend),
            AntiAliasing::Enabled,
        );
    }
}

/// The area an image placed in `rect` fills in a content box `size` in
/// size: `rect` on a padded axis, and the whole box on an axis tiling it.
fn extended(rect: Rect, size: kurbo::Size, extend: ImageExtend) -> Rect {
    let tiles = |extend: Extend| extend != Extend::Pad;
    let (x0, x1) = if tiles(extend.x) {
        (0.0, size.width)
    } else {
        (rect.x0, rect.x1)
    };
    let (y0, y1) = if tiles(extend.y) {
        (0.0, size.height)
    } else {
        (rect.y0, rect.y1)
    };

    Rect::new(x0, y0, x1, y1)
}

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;
