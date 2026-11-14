use comet_layout::fragment::{FragmentKey, tree::FragmentTree};
use comet_slotmap_tree::SlotTree;
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
    /// The layers, each a child of the layer it paints in. Children are kept
    /// in paint order, and the root layer has no parent.
    ///
    /// Kept in step with the layer of each owner, which
    /// [`Self::layer_of`] looks up. Changes made outside the builder, such as
    /// deleting a layer or changing its owner, must keep them in step.
    pub layers: SlotTree<PaintLayerKey, PaintLayerItem>,
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

    /// The layer a fragment starts, or `None` if it starts none.
    pub fn layer_of(&self, owner: FragmentKey) -> Option<PaintLayerKey> {
        self.by_owner.get(owner).copied()
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

    pub fn clear(&mut self) {
        self.layers.clear();
        self.by_owner.clear();
    }
}
