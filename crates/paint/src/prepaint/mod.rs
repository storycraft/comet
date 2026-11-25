//! Paint data resolved for a fragment tree before painting.
//!
//! A [`PrepaintTree`](tree::PrepaintTree) holds the [`FragmentData`] of every
//! box fragment, such as its transform relative to its parent box, and the
//! [`LayoutInputData`] of every layout input item. Inline items placing no
//! box, such as lines and glyph runs, have no data: they paint at their
//! location in the box owning them. Data only depends on the box or item
//! itself, and box data on its item, so any box can be resolved again on its
//! own: mark its item
//! [`DirtyFlags::NEED_PRE_PAINT`] with
//! [`DirtyMap::mark`](comet_layout::dirty_map::DirtyMap::mark)
//! and resolve from the root.
//! Inherited paint styles, such as text colors, are looked up while painting
//! instead.

pub mod builder;
pub mod fragment;
pub mod tree;

#[cfg(doc)]
use comet_layout::dirty_map::DirtyFlags;
use comet_layout::fragment::{BoxFragment, BoxFragmentType};
use kurbo::{Affine, Point, Rect, RoundedRect, RoundedRectRadii, Vec2};

/// Paint data of a layout input item, shared by all fragments it generates.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutInputData {
    /// Axes the item clips its overflow on. Its box fragment clips its
    /// contents to its own border box; inline part and anonymous boxes do not
    /// clip.
    /// Hiding or scrolling the overflow on one axis clips both, while
    /// `Overflow::Clip` only clips its own axis.
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

    /// Scroll offset the item's box fragment applies to its descendants,
    /// from its [`ScrollLeft`](crate::style::ScrollLeft) and
    /// [`ScrollTop`](crate::style::ScrollTop), clamped to the range its
    /// content overflows the padding box by, towards the right and bottom.
    /// Zero unless the item is a scroll container, scrolling or hiding its
    /// overflow on an axis, with a box fragment.
    ///
    /// A scroll container never splits, so it generates a single box
    /// fragment the offset applies to. The range is its
    /// [scroll range](comet_layout::fragment::FragmentTree::scroll_range),
    /// which follows its overflow. The fragment builder marks
    /// fragments whose overflow changed, so the offset is resolved again.
    pub scroll: Vec2,
}

/// Paint data of a box fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentData {
    /// Maps the box's local coordinates, with its border box's top-left
    /// corner at the origin, to the local coordinates of its parent box.
    /// A [principal](fragment::BoxRole::Principal) box applies its own
    /// [`Transform`](crate::style::Transform) around its transform origin,
    /// then is placed at its layout location:
    /// `translate(location + origin) * transform * translate(-origin)`.
    /// Other boxes are only placed.
    /// Ancestor transforms are not included: multiply them from the root
    /// down to map into scene coordinates.
    pub transform: Affine,

    /// Inverse of [`Self::transform`], mapping the parent fragment's local
    /// coordinates back to the fragment's, such as for hit testing. `None`
    /// when the transform is not invertible, such as when it scales an axis
    /// to zero: the fragment and its contents then cover no area in the
    /// scene.
    pub inverse_transform: Option<Affine>,

    /// Corner radii of the fragment's border box, resolved from its item's
    /// [`BorderRadius`](crate::style::BorderRadius) with its font size,
    /// against the fragment's own size: percentages resolve against its
    /// smaller side. All four corners are resolved, also for split
    /// fragments, such as for outlines. Backgrounds, borders, clips and hit
    /// testing square the corners joining a linked fragment, see
    /// [`Self::border_radii`].
    ///
    /// `None` for [anonymous](BoxFragmentType::Anonymous) boxes, which paint
    /// nothing, and items setting no radius.
    pub border_radius: Option<RoundedRectRadii>,
}

impl FragmentData {
    /// Corner radii of the border of `fragment`, the box this data was
    /// resolved for: its [border radius](Self::border_radius), with the
    /// corners joining a linked box square. Inline part boxes continue to the
    /// right of their previous box, other boxes below it. Square if it has no
    /// border radius.
    pub fn border_radii(&self, fragment: &BoxFragment) -> RoundedRectRadii {
        self.border_radius
            .map_or(RoundedRectRadii::from(0.0), |radii| {
                split_radii(fragment, radii)
            })
    }

    /// The border box of `fragment`, the box this data was resolved for, in
    /// its local coordinates, rounded with its
    /// [border radii](Self::border_radii).
    pub fn border_rect(&self, fragment: &BoxFragment) -> RoundedRect {
        let size = fragment.layout.geometry.size;

        Rect::from_origin_size(Point::ORIGIN, (size.width, size.height))
            .to_rounded_rect(self.border_radii(fragment))
    }
}

/// `radii` of `fragment`, with the corners joining a linked fragment square.
fn split_radii(fragment: &BoxFragment, mut radii: RoundedRectRadii) -> RoundedRectRadii {
    // Inline part boxes continue to the right of their previous box, other
    // boxes below it.
    let vertical = fragment.box_type != BoxFragmentType::InlinePart;

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
