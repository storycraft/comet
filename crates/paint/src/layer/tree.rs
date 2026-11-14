use std::ops::Index;

use slotmap::SlotMap;

use crate::layer::{PaintLayerItem, PaintLayerKey};

/// Paint layers of the fragments under a root fragment.
///
/// Built by a [`PaintLayerBuilder`](super::builder::PaintLayerBuilder), which
/// returns the root layer owned by the root fragment. The tree does not keep
/// it: callers hold on to it to paint or walk the layers.
#[derive(Debug, Default)]
pub struct PaintLayerTree {
    pub(super) items: SlotMap<PaintLayerKey, PaintLayerItem>,
}

impl PaintLayerTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: PaintLayerKey) -> Option<&PaintLayerItem> {
        self.items.get(key)
    }

    /// Every layer, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (PaintLayerKey, &PaintLayerItem)> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

impl Index<PaintLayerKey> for PaintLayerTree {
    type Output = PaintLayerItem;

    fn index(&self, key: PaintLayerKey) -> &PaintLayerItem {
        &self.items[key]
    }
}
