//! Reactive bindings of the image style props: their [`ReactiveProp`]
//! hints and the [`ImageStyled`] helpers.

use comet_reactivity::{
    prop::{PropHint, ReactiveProp},
    style::Styled,
};
use peniko::ImageQuality;
use reactive_graph::wrappers::read::Signal;

use crate::style::{ImageExtend, ImageRendering};

/// Style helpers for the [image style props](crate::style), next to the
/// [`Styled`] helpers. Each takes a plain value, a signal, a memo or a
/// [`Signal::derive`] closure, and the prop follows it.
pub trait ImageStyled: Styled {
    /// How the image's pixels are sampled when it is drawn scaled, see
    /// [`ImageRendering`].
    fn image_rendering(self, value: impl Into<Signal<ImageQuality>>) -> Self {
        self.bind::<ImageRendering, ImageQuality>(value)
    }

    /// How an image drawn smaller than its box extends past its edges
    /// on each axis, see [`ImageExtend`].
    fn image_extend(self, value: impl Into<Signal<ImageExtend>>) -> Self {
        self.bind::<ImageExtend, ImageExtend>(value)
    }
}

impl<S: Styled> ImageStyled for S {}

// Read by contents when they draw, so changes only paint again.
impl ReactiveProp for ImageRendering {
    fn hint(&self, _: Option<&Self>) -> PropHint {
        PropHint::RENDER
    }
}

impl ReactiveProp for ImageExtend {
    fn hint(&self, _: Option<&Self>) -> PropHint {
        PropHint::RENDER
    }
}
