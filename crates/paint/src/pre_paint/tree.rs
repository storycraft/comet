use std::ops::Index;

use comet_layout::{
    fragment::{FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
};
use slotmap::SecondaryMap;

use crate::pre_paint::{FragmentPosition, LayoutInputData};

/// Absolute positions of the fragments of a fragment tree, and the paint data
/// of the layout input items they were built from.
///
/// Filled by a [`PrePaintTreeResolver`](super::resolver::PrePaintTreeResolver).
#[derive(Default)]
pub struct PrePaintTree {
    pub(super) positions: SecondaryMap<FragmentKey, FragmentPosition>,
    pub(super) layout_mapping: SecondaryMap<LayoutInputKey, LayoutInputData>,
}

impl PrePaintTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// The absolute position a fragment was resolved to.
    pub fn position(&self, key: FragmentKey) -> Option<FragmentPosition> {
        self.positions.get(key).copied()
    }

    /// The paint data a layout input item was resolved to.
    pub fn get(&self, key: LayoutInputKey) -> Option<&LayoutInputData> {
        self.layout_mapping.get(key)
    }

    /// Drop the positions of fragments deleted from `fragments`.
    pub(super) fn retain_fragments(&mut self, fragments: &FragmentTree) {
        self.positions
            .retain(|fragment, _| fragments.items.get(fragment).is_some());
    }

    /// Drop the data of items deleted from `inputs`.
    pub(super) fn retain_items(&mut self, inputs: &LayoutInputTree) {
        self.layout_mapping
            .retain(|item, _| inputs.items.get(item).is_some());
    }

    pub fn clear(&mut self) {
        self.positions.clear();
        self.layout_mapping.clear();
    }
}

impl Index<FragmentKey> for PrePaintTree {
    type Output = FragmentPosition;

    fn index(&self, key: FragmentKey) -> &FragmentPosition {
        &self.positions[key]
    }
}

impl Index<LayoutInputKey> for PrePaintTree {
    type Output = LayoutInputData;

    fn index(&self, key: LayoutInputKey) -> &LayoutInputData {
        &self.layout_mapping[key]
    }
}
