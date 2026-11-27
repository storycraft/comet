//! External contents drawn by replaced boxes.
//!
//! A box whose `DisplayInner` is `Content(id)` is a replaced element: it lays
//! out no children, and is sized and drawn by the [`Content`] its
//! [`ContentId`] names in a [`ContentManager`](manager::ContentManager).
//!
//! A content keeps no state of the boxes drawing it. Layout supplies the
//! sizing constraints of each box in a [`MeasureInput`], and paint the size of
//! its content box in a [`DrawInput`], so one content can be shared by any
//! number of boxes. Both carry the [`Props`] of the box's node, so a content
//! can read style props, such as the [content styles](style) fitting it into
//! the box, which [`ObjectStyle`](style::ObjectStyle) measures and places
//! contents with.
//!
//! Every content draws into the scene of the layer its box paints in, images
//! and external textures alike. Images are provided by
//! `comet-content-image`, and external textures, such as videos or 3D scenes
//! rendered outside the UI, by `comet-content-external`.

pub mod manager;
pub mod prelude;
pub mod style;

use std::num::NonZeroU64;

use anyrender::Scene;
use comet_geometry::UnitResolveContext;
use comet_tree::ui::Props;

/// Opaque identifier of a [`Content`] in a [`ContentManager`](manager::ContentManager).
///
/// Ids are never reused by a manager, so a box keeping the id of a removed
/// content draws nothing rather than another content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentId(NonZeroU64);

impl ContentId {
    pub const fn from_ffi(id: NonZeroU64) -> Self {
        Self(id)
    }

    pub const fn as_ffi(self) -> NonZeroU64 {
        self.0
    }
}

/// Sizing constraints of a content box, supplied by layout, like the
/// arguments of a Taffy leaf measure function. Sizes are of the content box,
/// in logical units.
#[derive(Clone, Copy)]
pub struct MeasureInput<'a> {
    /// Props of the node of the box being measured.
    pub props: Props<'a>,
    /// Sizes the box already has on each axis, such as from its style or a
    /// stretching parent.
    pub known_dimensions: taffy::Size<Option<f32>>,
    /// Space available to the box on each axis.
    pub available_space: taffy::Size<taffy::AvailableSpace>,
    /// Size of the box's parent, if known.
    pub parent_size: taffy::Size<Option<f32>>,
}

/// The box a content draws into, supplied by paint.
#[derive(Clone)]
pub struct DrawInput<'a> {
    /// Props of the node of the box being drawn.
    pub props: Props<'a>,
    /// Size of the content box, in logical units. The content draws in the
    /// rectangle from the origin to this size.
    pub size: kurbo::Size,
    /// Physical pixels per logical unit of the display, such as to pick the
    /// resolution of the drawing.
    pub scale_factor: f64,
    /// Font and viewport sizes of the box, resolving the relative lengths of
    /// its style props, such as its
    /// [`ObjectPosition`](style::ObjectPosition).
    pub units: UnitResolveContext,
}

/// An external content drawn by replaced boxes.
///
/// It sizes and draws itself from the inputs alone, keeping no state of the
/// boxes using it, so it can be shared across them. Style props it reads
/// from the inputs' [`Props`] are its own to define.
pub trait Content {
    /// Size of the content box under `input`, in logical units. Taffy
    /// applies the box's padding, border, min and max sizes and aspect ratio
    /// around it.
    fn measure(&self, input: &MeasureInput<'_>) -> taffy::Size<f32>;

    /// Draw the content into the content box described by `input`, in its
    /// local coordinates.
    fn draw(&self, input: &DrawInput<'_>, scene: &mut Scene);
}

/// Brush transform mapping a source `natural` pixels in size, such as an
/// image or texture, onto the content box `size`. Identity for an empty source.
pub fn fit_transform(natural: kurbo::Size, size: kurbo::Size) -> kurbo::Affine {
    if natural.width <= 0.0 || natural.height <= 0.0 {
        return kurbo::Affine::IDENTITY;
    }

    kurbo::Affine::scale_non_uniform(size.width / natural.width, size.height / natural.height)
}

#[cfg(test)]
mod tests;
