use std::ops::Index;

use comet_layout::{fragment::FragmentKey, input::LayoutInputKey};
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
