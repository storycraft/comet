use std::ops::Index;

#[cfg(doc)]
use comet_layout::dirty_map::DirtyFlags;
use comet_layout::{
    fragment::{FragmentKey, tree::FragmentTree},
    input::LayoutInputKey,
};
use kurbo::{Point, Shape};
use slotmap::{SecondaryMap, SparseSecondaryMap};

use crate::pre_paint::{FragmentData, LayoutInputData};

/// Paint data of the fragments of a fragment tree, such as their transforms
/// relative to their parents, and of the layout input items they were built
/// from.
///
/// Filled by a [`PrePaintTreeBuilder`](super::builder::PrePaintTreeBuilder),
/// which resolves the fragments of the layout input items marked
/// [`DirtyFlags::NEED_PRE_PAINT`], including those the fragment builder laid
/// out, and the fragments without data.
/// Keys are generational, so data left behind by deleted fragments and items
/// is never returned for the fragments and items reusing their slots, and is
/// overwritten when those are resolved.
#[derive(Default)]
pub struct PrePaintTree {
    pub(super) fragment_mapping: SecondaryMap<FragmentKey, FragmentData>,
    pub(super) layout_mapping: SecondaryMap<LayoutInputKey, LayoutInputData>,
    /// Items whose paint values, or those of their fragments, use `vw` or
    /// `vh`, resolved again when the viewport changes. Few items use them,
    /// so the map is sparse. Keyed by item, since the fragments of an item
    /// share its props, and line fragments are replaced by layouts.
    pub(super) viewport_dependents: SparseSecondaryMap<LayoutInputKey, ()>,
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
