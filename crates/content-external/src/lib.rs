//! External texture contents for comet: textures rendered outside the UI,
//! such as video frames or 3D scenes, drawn by replaced boxes, see
//! [`comet_content`].

use anyrender::{AntiAliasing, Paint, PaintScene, ResourceId, Scene};
use kurbo::{Affine, Point, Rect};
use peniko::{Fill, ImageBrush, ImageSampler};

use comet_content::{
    Content, ContentKind, DrawInput, MeasureInput, fit_transform, natural_measure,
};

/// An external texture, such as a video frame or a 3D scene, registered with
/// the render context, stretched over the content box.
///
/// The texture is rendered and updated outside the UI: a new frame needs a
/// redraw, but no layout or paint of the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureContent {
    /// The texture, registered with
    /// [`RenderContext::try_register_custom_resource`](anyrender::RenderContext::try_register_custom_resource).
    pub resource: ResourceId,
    pub sampler: ImageSampler,
    /// Pixel size of the texture.
    pub pixel_size: kurbo::Size,
    /// Natural size in logical units: the pixel size by default.
    pub natural_size: kurbo::Size,
}

impl TextureContent {
    pub fn new(resource: ResourceId, width: u32, height: u32) -> Self {
        let pixel_size = kurbo::Size::new(f64::from(width), f64::from(height));

        Self {
            resource,
            sampler: ImageSampler::default(),
            pixel_size,
            natural_size: pixel_size,
        }
    }

    /// Set the natural size, such as for a texture of a higher density.
    pub fn with_natural_size(mut self, size: kurbo::Size) -> Self {
        self.natural_size = size;
        self
    }
}

impl Content for TextureContent {
    fn kind(&self) -> ContentKind {
        ContentKind::Texture {
            resource: self.resource,
        }
    }

    fn measure(&self, input: &MeasureInput<'_>) -> taffy::Size<f32> {
        natural_measure(self.natural_size, input)
    }

    fn draw(&self, input: &DrawInput<'_>, scene: &mut Scene) {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Paint::Resource(ImageBrush {
                image: self.resource,
                sampler: self.sampler,
            }),
            Some(fit_transform(self.pixel_size, input.size)),
            &Rect::from_origin_size(Point::ORIGIN, input.size),
            AntiAliasing::Enabled,
        );
    }
}

#[cfg(test)]
mod fixture;
#[cfg(test)]
mod tests;
