use std::ops::Index;

#[cfg(doc)]
use comet_layout::input::InputFlags;
use comet_layout::{
    fragment::{FragmentKey, tree::FragmentTree},
    input::LayoutInputKey,
};
use kurbo::{Point, Shape};
use slotmap::SecondaryMap;

use crate::pre_paint::{FragmentData, LayerFlags, LayoutInputData};

/// Paint data of the fragments of a fragment tree, such as their transforms
/// relative to their parents, and of the layout input items they were built
/// from.
///
/// Filled by a [`PrePaintTreeResolver`](super::resolver::PrePaintTreeResolver),
/// which resolves the fragments of the layout input items marked
/// [`InputFlags::NEED_PRE_PAINT`] and the fragments without data.
/// Keys are generational, so data left behind by deleted fragments and items
/// is never returned for the fragments and items reusing their slots, and is
/// overwritten when those are resolved.
#[derive(Default)]
pub struct PrePaintTree {
    pub(super) fragment_mapping: SecondaryMap<FragmentKey, FragmentData>,
    pub(super) layout_mapping: SecondaryMap<LayoutInputKey, LayoutInputData>,
    /// Layer changes of fragments. Fragments without an entry are unchanged.
    pub(super) layer_flags: SecondaryMap<FragmentKey, LayerFlags>,
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

    /// The layer changes of a fragment, set by resolves since the last
    /// [`Self::clear_layer_flags`].
    pub fn layer_flags(&self, key: FragmentKey) -> LayerFlags {
        self.layer_flags.get(key).copied().unwrap_or_default()
    }

    /// Clear the layer flags of every fragment. Layer builds keep them, so
    /// run it after building layers.
    pub fn clear_layer_flags(&mut self) {
        self.layer_flags.clear();
    }

    /// Set the layer change of a fragment, [`LayerFlags::NEED_LAYER`] or
    /// [`LayerFlags::NEED_NO_LAYER`], replacing the other, and mark its
    /// ancestors [`LayerFlags::CHILD_LAYER_UPDATE`].
    pub(super) fn mark_layer(
        &mut self,
        fragments: &FragmentTree,
        key: FragmentKey,
        change: LayerFlags,
    ) {
        let Some(entry) = self.layer_flags.entry(key) else {
            return;
        };
        let flags = entry.or_default();
        flags.remove(LayerFlags::NEED_LAYER | LayerFlags::NEED_NO_LAYER);
        flags.insert(change);

        let mut ancestor = fragments.items.parent(key);

        while let Some(key) = ancestor {
            let Some(entry) = self.layer_flags.entry(key) else {
                break;
            };
            let flags = entry.or_default();

            // Ancestors of a marked fragment are marked already.
            if flags.contains(LayerFlags::CHILD_LAYER_UPDATE) {
                break;
            }

            flags.insert(LayerFlags::CHILD_LAYER_UPDATE);
            ancestor = fragments.items.parent(key);
        }
    }

    pub fn clear(&mut self) {
        self.fragment_mapping.clear();
        self.layout_mapping.clear();
        self.layer_flags.clear();
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
