use std::ops::Index;

use comet_layout::fragment::{FragmentKey, tree::FragmentTree};
use comet_slotmap_tree::{SlotTree, iter::Cursor};
use slotmap::SecondaryMap;

use crate::layer::{PaintLayerItem, PaintLayerKey};

/// Paint layers of the fragments under a root fragment, as a tree of layers.
///
/// The child layers of each layer are kept in paint order: sorted by z index,
/// with equal z indices in fragment tree order. Layers only refer to their
/// owner fragments. The fragments painting in a layer are found by walking
/// its owner's subtree, skipping the owners of child layers, so changes that
/// keep the set of owners leave the tree as it is.
///
/// Built and updated by a
/// [`PaintLayerBuilder`](super::builder::PaintLayerBuilder), which returns
/// the root layer owned by the root fragment. The tree does not keep it:
/// callers hold on to it to paint, hit test or walk the layers. Layers keep
/// their keys across builds for as long as their owner starts one.
#[derive(Debug)]
pub struct PaintLayerTree {
    pub(super) layers: SlotTree<PaintLayerKey, PaintLayerItem>,
    /// The layer each owner fragment starts.
    pub(super) by_owner: SecondaryMap<FragmentKey, PaintLayerKey>,
}

impl Default for PaintLayerTree {
    fn default() -> Self {
        Self {
            layers: SlotTree::new(),
            by_owner: SecondaryMap::new(),
        }
    }
}

impl PaintLayerTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: PaintLayerKey) -> Option<&PaintLayerItem> {
        self.layers.get(key)
    }

    /// The layer a fragment starts, or `None` if it starts none.
    pub fn layer_of(&self, owner: FragmentKey) -> Option<PaintLayerKey> {
        self.by_owner.get(owner).copied().filter(|&layer| {
            self.layers
                .get(layer)
                .is_some_and(|item| item.owner == owner)
        })
    }

    /// The layer a fragment paints in: the layer of its nearest ancestor or
    /// self starting one, or `None` if none does.
    pub fn enclosing(
        &self,
        fragments: &FragmentTree,
        fragment: FragmentKey,
    ) -> Option<PaintLayerKey> {
        let mut next = Some(fragment);

        while let Some(fragment) = next {
            if let Some(layer) = self.layer_of(fragment) {
                return Some(layer);
            }

            next = fragments.items.parent(fragment);
        }

        None
    }

    /// The layer a layer is a child of, or `None` for the root layer.
    pub fn parent(&self, key: PaintLayerKey) -> Option<PaintLayerKey> {
        self.layers.parent(key)
    }

    /// The child layers of a layer, in paint order.
    pub fn children(&self, key: PaintLayerKey) -> Cursor<'_, PaintLayerKey, PaintLayerItem> {
        self.layers.children(key)
    }

    pub fn len(&self) -> usize {
        self.layers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    pub fn clear(&mut self) {
        self.layers.clear();
        self.by_owner.clear();
    }
}

impl Index<PaintLayerKey> for PaintLayerTree {
    type Output = PaintLayerItem;

    fn index(&self, key: PaintLayerKey) -> &PaintLayerItem {
        &self.layers[key]
    }
}
