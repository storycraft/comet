use std::ops::Index;

#[cfg(doc)]
use comet_layout::dirty_map::DirtyFlags;
use comet_layout::{
    fragment::{BoxFragment, BoxFragmentKey, FragmentTree},
    input::LayoutInputKey,
};
use kurbo::{Point, Shape, Vec2};
use slotmap::{SecondaryMap, SparseSecondaryMap};

use crate::prepaint::{FragmentData, LayoutInputData, fragment::BoxRole};

/// Paint data of the box fragments of a fragment tree, such as their
/// transforms relative to their parents, and of the layout input items they
/// were built from.
///
/// Filled by a [`PrepaintTreeBuilder`](super::builder::PrepaintTreeBuilder),
/// which resolves the boxes of the layout input items marked
/// [`DirtyFlags::NEED_PRE_PAINT`], including those the fragment builder laid
/// out, and the boxes without data.
/// Keys are generational, so data left behind by deleted boxes and items
/// is never returned for the boxes and items reusing their slots, and is
/// overwritten when those are resolved.
#[derive(Default)]
pub struct PrepaintTree {
    pub(super) fragment_mapping: SecondaryMap<BoxFragmentKey, FragmentData>,
    pub(super) layout_mapping: SecondaryMap<LayoutInputKey, LayoutInputData>,
    /// Items whose paint values, or those of their fragments, use `vw` or
    /// `vh`, resolved again when the viewport changes. Few items use them,
    /// so the map is sparse. Keyed by item, since the fragments of an item
    /// share its props, and inline part boxes are replaced by layouts.
    pub(super) viewport_dependents: SparseSecondaryMap<LayoutInputKey, ()>,
}

impl PrepaintTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// The paint data a box fragment was resolved to.
    pub fn fragment(&self, key: BoxFragmentKey) -> Option<&FragmentData> {
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

    /// The scroll offset `fragment` applies to its descendants: the
    /// [scroll](LayoutInputData::scroll) of its item for
    /// [principal](BoxRole::Principal) boxes, zero for others and for
    /// unresolved items.
    pub fn scroll_offset(&self, fragment: &BoxFragment) -> Vec2 {
        if !BoxRole::of(fragment).is_principal() {
            return Vec2::ZERO;
        }

        self.get(fragment.source)
            .map_or(Vec2::ZERO, |data| data.scroll)
    }

    /// Whether `point`, in the local coordinates of a box fragment, hits the
    /// box itself: it falls in its
    /// [border rect](FragmentData::border_rect), square at the joins of a
    /// split box.
    ///
    /// Boxes not in `fragments`, or not resolved in `self`, are never hit.
    pub fn hit_local(&self, fragments: &FragmentTree, key: BoxFragmentKey, point: Point) -> bool {
        let Some(fragment) = fragments.get(key) else {
            return false;
        };

        let Some(data) = self.fragment(key) else {
            return false;
        };

        data.border_rect(fragment).contains(point)
    }
}

impl Index<BoxFragmentKey> for PrepaintTree {
    type Output = FragmentData;

    fn index(&self, key: BoxFragmentKey) -> &FragmentData {
        &self.fragment_mapping[key]
    }
}

impl Index<LayoutInputKey> for PrepaintTree {
    type Output = LayoutInputData;

    fn index(&self, key: LayoutInputKey) -> &LayoutInputData {
        &self.layout_mapping[key]
    }
}
