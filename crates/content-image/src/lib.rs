//! Image contents for comet: raster images drawn by replaced boxes, see
//! [`comet_content`].

use anyrender::{AntiAliasing, Paint, PaintScene, Scene};
use kurbo::Affine;
use peniko::{Fill, ImageBrush, ImageData};

use comet_content::{Content, DrawInput, MeasureInput, fit_transform, style::ObjectStyle};

/// A raster image, sized and placed in its box by the
/// [content styles](comet_content::style) of the box's node.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageContent {
    pub brush: ImageBrush,
    /// Natural size in logical units: the image's pixel size by default.
    pub natural_size: kurbo::Size,
}

impl ImageContent {
    pub fn new(image: ImageData) -> Self {
        Self::from_brush(ImageBrush::new(image))
    }

    pub fn from_brush(brush: ImageBrush) -> Self {
        let natural_size =
            kurbo::Size::new(f64::from(brush.image.width), f64::from(brush.image.height));

        Self {
            brush,
            natural_size,
        }
    }

    /// Set the natural size, such as for an image of a higher density.
    pub fn with_natural_size(mut self, size: kurbo::Size) -> Self {
        self.natural_size = size;
        self
    }

    /// Pixel size of the image.
    fn pixel_size(&self) -> kurbo::Size {
        kurbo::Size::new(
            f64::from(self.brush.image.width),
            f64::from(self.brush.image.height),
        )
    }
}

impl Content for ImageContent {
    fn measure(&self, input: &MeasureInput<'_>) -> taffy::Size<f32> {
        ObjectStyle::of(input.props).measure(self.natural_size, input)
    }

    fn draw(&self, input: &DrawInput<'_>, scene: &mut Scene) {
        let rect = ObjectStyle::of(input.props).place(self.natural_size, input);

        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Paint::Image(self.brush.as_ref()),
            Some(
                Affine::translate(rect.origin().to_vec2())
                    * fit_transform(self.pixel_size(), rect.size()),
            ),
            &rect,
            AntiAliasing::Enabled,
        );
    }
}

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;
