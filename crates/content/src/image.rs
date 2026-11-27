//! Image contents.

use anyrender::{AntiAliasing, Paint, PaintScene, Scene};
use kurbo::{Affine, Point, Rect};
use peniko::{Fill, ImageBrush, ImageData};

use crate::{Content, ContentKind, DrawInput, MeasureInput, fit_transform, natural_measure};

/// A raster image, stretched over the content box.
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
    fn kind(&self) -> ContentKind {
        ContentKind::Scene
    }

    fn measure(&self, input: &MeasureInput) -> taffy::Size<f32> {
        natural_measure(self.natural_size, input)
    }

    fn draw(&self, input: &DrawInput, scene: &mut Scene) {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Paint::Image(self.brush.as_ref()),
            Some(fit_transform(self.pixel_size(), input.size)),
            &Rect::from_origin_size(Point::ORIGIN, input.size),
            AntiAliasing::Enabled,
        );
    }
}
