use std::ops::Index;

use comet_layout::fragment::FragmentKey;
use slotmap::{SlotMap, new_key_type};

new_key_type! {
    pub struct PaintLayerKey;
}

/// One entry painted by a layer, in paint order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintLayerEntry {
    /// A fragment painting in the layer, without its descendants: they have
    /// entries of their own.
    Fragment(FragmentKey),
    /// A child layer, painted whole.
    Layer(PaintLayerKey),
}

/// A layer of fragments painting together, owned by a fragment.
#[derive(Debug, Clone)]
pub struct PaintLayerItem {
    /// Fragment starting the layer. Its pre-paint data gives the transform and
    /// opacity the layer paints with.
    pub owner: FragmentKey,

    /// Layer the owner fragment would paint in without starting its own.
    pub parent: Option<PaintLayerKey>,

    /// Z index of the layer among the entries of its parent layer.
    pub z_index: i32,

    /// Fragments painting in the layer and its child layers, in paint order.
    ///
    /// The owner fragment comes first. The rest are sorted by z index:
    /// fragments sort at 0, child layers at their own z index, and entries
    /// with equal z index keep their fragment tree order.
    pub entries: Vec<PaintLayerEntry>,
}

/// Paint layers of the fragments under a root fragment.
///
/// Built by a [`PaintLayerBuilder`](super::builder::PaintLayerBuilder).
#[derive(Debug, Default)]
pub struct PaintLayerTree {
    pub(super) items: SlotMap<PaintLayerKey, PaintLayerItem>,
    pub(super) root: Option<PaintLayerKey>,
}

impl PaintLayerTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// The layer owned by the root fragment, or `None` before a build.
    pub fn root(&self) -> Option<PaintLayerKey> {
        self.root
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
        self.root = None;
    }
}

impl Index<PaintLayerKey> for PaintLayerTree {
    type Output = PaintLayerItem;

    fn index(&self, key: PaintLayerKey) -> &PaintLayerItem {
        &self.items[key]
    }
}
