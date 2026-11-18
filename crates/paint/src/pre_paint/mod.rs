//! Paint data resolved for a fragment tree before painting.
//!
//! A [`PrePaintTree`](tree::PrePaintTree) holds the [`FragmentData`] of every
//! fragment, such as its transform relative to its parent fragment, and the
//! [`LayoutInputData`] of every layout input item. Data only depends on the
//! fragment or item itself, and fragment data on its item, so any fragment can
//! be resolved again on its own: mark its item
//! [`DirtyFlags::NEED_PRE_PAINT`] with
//! [`DirtyMap::mark`](comet_layout::dirty_map::DirtyMap::mark)
//! and resolve from the root.
//! Inherited paint styles, such as text colors, are looked up while painting
//! instead.

pub mod builder;
pub mod tree;

#[cfg(doc)]
use comet_layout::dirty_map::DirtyFlags;
use comet_layout::fragment::Fragment;
use kurbo::{Affine, Point, Rect, RoundedRect, RoundedRectRadii};

/// Paint data of a layout input item, shared by all fragments it generates.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutInputData {
    /// Axes the item clips its overflow on. Each box fragment of the item
    /// clips its contents to its own border box; other fragments do not clip.
    pub clip: (bool, bool),

    /// Width of the outline drawn around the item's fragments, resolved from
    /// its [`OutlineWidth`](crate::style::OutlineWidth) with its font size.
    /// Percentages resolve to 0. At least 0, and 0 if the item sets none.
    pub outline_width: f64,

    /// Distance between the outline and the border box, outwards, resolved
    /// from its [`OutlineOffset`](crate::style::OutlineOffset) with its font
    /// size. Negative offsets draw the outline inside the box. Percentages
    /// resolve to 0, and so does an item setting none.
    pub outline_offset: f64,
}

/// Paint data of a fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentData {
    /// Maps the fragment's local coordinates, with its border box's top-left
    /// corner at the origin, to the local coordinates of its parent fragment.
    /// Applies its own [`Transform`](crate::style::Transform) around
    /// [`Self::transform_origin`], then places the fragment at its layout
    /// location: `translate(location + origin) * transform *
    /// translate(-origin)`. Ancestor transforms are not included: multiply
    /// them from the root down to map into scene coordinates.
    pub transform: Affine,

    /// Inverse of [`Self::transform`], mapping the parent fragment's local
    /// coordinates back to the fragment's, such as for hit testing. `None`
    /// when the transform is not invertible, such as when it scales an axis
    /// to zero: the fragment and its contents then cover no area in the
    /// scene.
    pub inverse_transform: Option<Affine>,

    /// Point the fragment's own transform is applied around, relative to its
    /// border box's top-left corner. The centre of its border box, unless a
    /// box sets [`TransformOrigin`](crate::style::TransformOrigin).
    pub transform_origin: Point,

    /// Z index of the paint layer the fragment starts, or `None` if it paints
    /// in the layer of its parent. See
    /// [`PaintLayerTree`](crate::layer::tree::PaintLayerTree).
    ///
    /// Only boxes start layers: when their item sets a
    /// [`Transform`](crate::style::Transform), an
    /// [`Opacity`](crate::style::Opacity), a
    /// [`Filter`](crate::style::Filter) or a
    /// [`BackdropFilter`](crate::style::BackdropFilter), whatever its value,
    /// or a [`ZIndex`](crate::style::ZIndex) other than auto. An auto z index
    /// counts as 0.
    pub stacking: Option<i32>,

    /// Corner radii of the fragment's border box, resolved from its item's
    /// [`BorderRadius`](crate::style::BorderRadius) with its font size,
    /// against the fragment's own size: percentages resolve against its
    /// smaller side. All four corners are resolved, also for split
    /// fragments, such as for outlines. Backgrounds, borders, clips and hit
    /// testing square the corners joining a linked fragment, see
    /// [`Self::border_radii`].
    ///
    /// `None` for fragments painting no box, such as line boxes and anonymous
    /// blocks, or items setting no radius.
    pub border_radius: Option<RoundedRectRadii>,
}

impl FragmentData {
    /// Corner radii of the border of `fragment`, the fragment this data was
    /// resolved for: its [border radius](Self::border_radius), with the
    /// corners joining a linked fragment square. Box fragments continue below
    /// their previous fragment, inline and glyph run fragments to its right.
    /// Square if it has no border radius.
    pub fn border_radii(&self, fragment: &Fragment) -> RoundedRectRadii {
        self.border_radius
            .map_or(RoundedRectRadii::from(0.0), |radii| {
                split_radii(fragment, radii)
            })
    }

    /// The border box of `fragment`, the fragment this data was resolved
    /// for, in its local coordinates, rounded with its
    /// [border radii](Self::border_radii).
    pub fn border_rect(&self, fragment: &Fragment) -> RoundedRect {
        let size = fragment.layout.size;

        Rect::from_origin_size(Point::ORIGIN, (size.width, size.height))
            .to_rounded_rect(self.border_radii(fragment))
    }
}

/// `radii` of `fragment`, with the corners joining a linked fragment square.
fn split_radii(fragment: &Fragment, mut radii: RoundedRectRadii) -> RoundedRectRadii {
    // Box fragments continue below their previous fragment, inline and glyph
    // run fragments to its right.
    let vertical = fragment.item.is_box();

    if fragment.link.prev.is_some() {
        radii.top_left = 0.0;

        if vertical {
            radii.top_right = 0.0;
        } else {
            radii.bottom_left = 0.0;
        }
    }

    if fragment.link.next.is_some() {
        radii.bottom_right = 0.0;

        if vertical {
            radii.bottom_left = 0.0;
        } else {
            radii.top_right = 0.0;
        }
    }

    radii
}
