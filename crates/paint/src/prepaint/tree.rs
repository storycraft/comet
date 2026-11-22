use std::ops::Index;

#[cfg(doc)]
use comet_layout::dirty_map::DirtyFlags;
use comet_layout::{
    fragment::{Fragment, FragmentKey, tree::FragmentTree},
    input::LayoutInputKey,
};
use kurbo::{Point, Shape, Vec2};
use slotmap::{SecondaryMap, SparseSecondaryMap};

use crate::prepaint::{FragmentData, LayoutInputData};

/// Paint data of the fragments of a fragment tree, such as their transforms
/// relative to their parents, and of the layout input items they were built
/// from.
///
/// Filled by a [`PrepaintTreeBuilder`](super::builder::PrepaintTreeBuilder),
/// which resolves the fragments of the layout input items marked
/// [`DirtyFlags::NEED_PRE_PAINT`], including those the fragment builder laid
/// out, and the fragments without data.
/// Keys are generational, so data left behind by deleted fragments and items
/// is never returned for the fragments and items reusing their slots, and is
/// overwritten when those are resolved.
#[derive(Default)]
pub struct PrepaintTree {
    pub(super) fragment_mapping: SecondaryMap<FragmentKey, FragmentData>,
    pub(super) layout_mapping: SecondaryMap<LayoutInputKey, LayoutInputData>,
    /// Items whose paint values, or those of their fragments, use `vw` or
    /// `vh`, resolved again when the viewport changes. Few items use them,
    /// so the map is sparse. Keyed by item, since the fragments of an item
    /// share its props, and line fragments are replaced by layouts.
    pub(super) viewport_dependents: SparseSecondaryMap<LayoutInputKey, ()>,
}

impl PrepaintTree {
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

    /// Whether the paint values of an item, or of its fragments, were
    /// resolved with `vw` or `vh` lengths.
    pub fn depends_viewport(&self, key: LayoutInputKey) -> bool {
        self.viewport_dependents.contains_key(key)
    }

    pub fn clear(&mut self) {
        self.fragment_mapping.clear();
        self.layout_mapping.clear();
        self.viewport_dependents.clear();
    }

    /// The scroll offset `fragment` applies to its descendants, clamped to
    /// its scrollable range: see [`LayoutInputData::scroll_offset`]. Zero if
    /// its item is not resolved.
    pub fn scroll_offset(&self, fragment: &Fragment) -> Vec2 {
        self.get(fragment.source)
            .map_or(Vec2::ZERO, |data| data.scroll_offset(fragment))
    }

    /// Whether `point`, in the local coordinates of a fragment, hits the
    /// fragment itself: it falls in its
    /// [border rect](FragmentData::border_rect), square at the joins of a
    /// split fragment.
    ///
    /// Fragments not in `fragments`, or not resolved in `self`, are never
    /// hit.
    pub fn hit_local(&self, fragments: &FragmentTree, key: FragmentKey, point: Point) -> bool {
        let Some(fragment) = fragments.items.get(key) else {
            return false;
        };

        let Some(data) = self.fragment(key) else {
            return false;
        };

        data.border_rect(fragment).contains(point)
    }
}

impl Index<FragmentKey> for PrepaintTree {
    type Output = FragmentData;

    fn index(&self, key: FragmentKey) -> &FragmentData {
        &self.fragment_mapping[key]
    }
}

impl Index<LayoutInputKey> for PrepaintTree {
    type Output = LayoutInputData;

    fn index(&self, key: LayoutInputKey) -> &LayoutInputData {
        &self.layout_mapping[key]
    }
}
