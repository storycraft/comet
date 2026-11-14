use std::ops::Index;

use comet_layout::{
    fragment::{FragmentKey, tree::FragmentTree},
    input::LayoutInputKey,
};
use kurbo::{Point, Rect, Shape};
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

    /// Whether `point`, in the local coordinates of a fragment, hits the
    /// fragment itself: it falls in its border box, rounded with its
    /// resolved border radius.
    ///
    /// Fragments not in `fragments`, or not resolved in `self`, are never
    /// hit.
    pub fn hit_local(&self, fragments: &FragmentTree, key: FragmentKey, point: Point) -> bool {
        let (Some(fragment), Some(data)) = (fragments.items.get(key), self.fragment(key)) else {
            return false;
        };

        let size = fragment.layout.size;
        let rect = Rect::from_origin_size(Point::ORIGIN, (size.width, size.height));
        let Some(radii) = data.border_radius else {
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
