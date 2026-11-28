//! Style props of image contents, read from the props of the box displaying
//! the image, next to the [content styles](comet_content::style) sizing and
//! placing it.

#[cfg(feature = "reactivity")]
pub mod reactive;

use comet_tree::style::{StyleProp, define_style_props};
use peniko::{Extend, ImageQuality, ImageSampler};

define_style_props!(
    /// How the image's pixels are sampled when it is drawn scaled: nearest
    /// neighbor, bilinear or bicubic. Defaults to [`ImageQuality::Medium`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub ImageRendering: ImageQuality = ImageQuality::Medium;
);

/// How an image drawn smaller than its box extends past its edges on each
/// axis. Defaults to [`Extend::Pad`] on both, which draws the image once.
///
/// [`Extend::Repeat`] and [`Extend::Reflect`] tile the image across the box
/// on their axis, starting from where it is
/// [placed](comet_content::style::ObjectStyle::place). A padded axis keeps
/// the image to its own size, rather than stretching its edge pixels.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ImageExtend {
    pub x: Extend,
    pub y: Extend,
}

impl StyleProp for ImageExtend {}

impl ImageExtend {
    /// The same extend on both axes.
    pub const fn both(extend: Extend) -> Self {
        Self {
            x: extend,
            y: extend,
        }
    }
}

/// The sampler drawing an image with the quality of `rendering` and the
/// extends of `extend`, opaque.
pub(crate) fn sampler(rendering: ImageRendering, extend: ImageExtend) -> ImageSampler {
    ImageSampler {
        x_extend: extend.x,
        y_extend: extend.y,
        quality: rendering.0,
        alpha: 1.0,
    }
}
