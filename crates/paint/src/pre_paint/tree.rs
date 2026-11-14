use std::ops::Index;

use comet_layout::{
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
    input::LayoutInputKey,
};
use kurbo::{Point, Rect, RoundedRectRadii, Shape};
use slotmap::SecondaryMap;

use crate::pre_paint::{FragmentData, LayoutInputData};

/// Paint data of the fragments of a fragment tree, such as their transforms
/// relative to their parents, and of the layout input items they were built
/// from.
///
/// Filled by a [`PrePaintTreeResolver`](super::resolver::PrePaintTreeResolver).
/// Keys are generational, so data left behind by deleted fragments and items
/// is never returned for the fragments and items reusing their slots, and is
/// overwritten when those are resolved.
#[derive(Default)]
pub struct PrePaintTree {
    pub(super) fragment_mapping: SecondaryMap<FragmentKey, FragmentData>,
    pub(super) layout_mapping: SecondaryMap<LayoutInputKey, LayoutInputData>,
}

impl PrePaintTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// The paint data a fragment was resolved to.
    pub fn fragment(&self, key: FragmentKey) -> Option<&FragmentData> {
        self.fragment_mapping.get(key)
    }

    /// The paint data a layout input item was resolved to.
    pub fn get(&self, key: LayoutInputKey) -> Option<&LayoutInputData> {
        self.layout_mapping.get(key)
    }

    pub fn clear(&mut self) {
        self.fragment_mapping.clear();
        self.layout_mapping.clear();
    }

    /// Corner radii of a fragment's border box: the
    /// [border radius](LayoutInputData::border_radius) of its item, with the
    /// corners joining a linked fragment square. Box fragments continue below
    /// their previous fragment, inline and glyph run fragments to its right.
    ///
    /// `None` for fragments painting no box, such as line boxes and anonymous
    /// blocks, items setting no radius, and fragments or items missing from
    /// `fragments` or `self`.
    pub fn border_radius(
        &self,
        fragments: &FragmentTree,
        key: FragmentKey,
    ) -> Option<RoundedRectRadii> {
        let fragment = fragments.items.get(key)?;

        if matches!(
            fragment.item,
            FragmentItem::Line { .. } | FragmentItem::AnonymousBlock { .. }
        ) {
            return None;
        }

        let radii = self.get(fragment.source)?.border_radius?;

        Some(split_radii(fragment, radii))
    }

    /// Whether `point`, in the local coordinates of a fragment, hits the
    /// fragment itself: it falls in its border box, rounded with its
    /// [border radius](Self::border_radius).
    ///
    /// Fragments not in `fragments`, or not resolved in `self`, are never
    /// hit.
    pub fn hit_local(&self, fragments: &FragmentTree, key: FragmentKey, point: Point) -> bool {
        let Some(fragment) = fragments.items.get(key) else {
            return false;
        };

        if self.fragment(key).is_none() {
            return false;
        }

        let size = fragment.layout.size;
        let rect = Rect::from_origin_size(Point::ORIGIN, (size.width, size.height));
        let Some(radii) = self.border_radius(fragments, key) else {
            return rect.contains(point);
        };

        rect.to_rounded_rect(radii).contains(point)
    }
}

impl Index<FragmentKey> for PrePaintTree {
    type Output = FragmentData;

    fn index(&self, key: FragmentKey) -> &FragmentData {
        &self.fragment_mapping[key]
    }
}

impl Index<LayoutInputKey> for PrePaintTree {
    type Output = LayoutInputData;

    fn index(&self, key: LayoutInputKey) -> &LayoutInputData {
        &self.layout_mapping[key]
    }
}

/// The corner radii of a fragment of an item with `radii`, with the corners
/// joining a linked fragment square.
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
