use std::ops::Index;

use comet_layout::{
    fragment::{FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
};
use slotmap::SecondaryMap;

use crate::pre_paint::{FragmentData, LayoutInputData};

/// Paint data of the fragments of a fragment tree, such as their absolute
/// positions, and of the layout input items they were built from.
///
/// Filled by a [`PrePaintTreeResolver`](super::resolver::PrePaintTreeResolver).
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

    /// Drop the data of fragments deleted from `fragments`.
    pub(super) fn retain_fragments(&mut self, fragments: &FragmentTree) {
        self.fragment_mapping
            .retain(|fragment, _| fragments.items.get(fragment).is_some());
    }

    /// Drop the data of items deleted from `inputs`.
    pub(super) fn retain_items(&mut self, inputs: &LayoutInputTree) {
        self.layout_mapping
            .retain(|item, _| inputs.items.get(item).is_some());
    }

    pub fn clear(&mut self) {
        self.fragment_mapping.clear();
        self.layout_mapping.clear();
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
