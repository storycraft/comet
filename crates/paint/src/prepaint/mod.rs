//! Paint data resolved for a fragment tree before painting.
//!
//! A [`PrepaintTree`](tree::PrepaintTree) holds the [`FragmentData`] of every
//! box fragment, such as its border radius, and the [`LayoutInputData`] of
//! every layout input item, text items included. Transforms and scroll
//! offsets are resolved into the [paint layers](crate::layer) instead, which
//! do not read pre-paint data. Inline items placing no box, such as lines and
//! glyph runs, have no data: they paint at their location in the box owning
//! them.
//!
//! Box data only depends on the box and its item's props, so a box is
//! resolved again on its own. Item data also inherits from the parent item,
//! such as text colors and percentage outlines, so an item resolved to new
//! inherited values resolves its children again too, down to the items whose
//! values stay the same. To resolve an item again, mark it
//! [`DirtyFlags::NEED_PRE_PAINT`] with
//! [`DirtyMap::mark`](comet_layout::dirty_map::DirtyMap::mark)
//! and resolve from the root.

pub mod builder;
pub mod fragment;
pub mod tree;

use anyrender::Paint;
#[cfg(doc)]
use comet_layout::dirty_map::DirtyFlags;
use comet_layout::fragment::{BoxFragment, BoxFragmentType};
use kurbo::{Point, Rect, RoundedRect, RoundedRectRadii};

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
    /// Percentages resolve against the parent item's outline width, 0 for
    /// the root. At least 0, and 0 if the item sets none.
    pub outline_width: f64,

    /// Distance between the outline and the border box, outwards, resolved
    /// from its [`OutlineOffset`](crate::style::OutlineOffset) with its font
    /// size. Negative offsets draw the outline inside the box. Percentages
    /// resolve against the parent item's outline offset, 0 for the root.
    /// 0 if the item sets none.
    pub outline_offset: f64,

    /// Color the item's glyph runs paint with: its
    /// [`TextColor`](crate::style::TextColor), or the parent item's text
    /// color when [auto](comet_layout::style::geometry::AutoValue::Auto).
    /// Black for the root. Inherited along the input item tree, not the
    /// fragment tree, so a block inside an inline span inherits from the
    /// span.
    pub text_color: Paint,
}

impl LayoutInputData {
    /// Whether `self` and `other` pass the same values down to the children
    /// of their item: those the children resolve with.
    pub fn inherits_same(&self, other: &Self) -> bool {
        self.text_color == other.text_color
            && self.outline_width == other.outline_width
            && self.outline_offset == other.outline_offset
    }
}

/// Paint data of a box fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentData {
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
