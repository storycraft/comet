use comet_layout::fragment::{FragmentKey, tree::FragmentTree};

use crate::{
    layer::{PaintLayerItem, PaintLayerKey, tree::PaintLayerTree},
    pre_paint::tree::PrePaintTree,
};

/// Builds and updates a [`PaintLayerTree`] for a fragment tree, from the
/// [stacking](crate::pre_paint::FragmentData::stacking) z indices resolved in
/// its [`PrePaintTree`]. Keep it to reuse its buffers: once the trees settle,
/// building again allocates nothing.
#[derive(Default)]
pub struct PaintLayerBuilder {
    /// Number of the current build or update, stamped on the layers it
    /// reaches.
    pass: u32,
    /// Fragments to place, with the layer they paint in and whether they are
    /// inside the subtree being rebuilt.
    pending: Vec<Visit>,
    /// Child layers detached from the layers being rebuilt. Those not
    /// reached again are deleted.
    detached: Vec<PaintLayerKey>,
    /// Layers to delete, children after their parents.
    deleting: Vec<PaintLayerKey>,
}

#[derive(Clone, Copy)]
struct Visit {
    fragment: FragmentKey,
    layer: PaintLayerKey,
    /// Whether the fragment is inside the subtree being rebuilt, so layers it
    /// starts are rebuilt too.
    inside: bool,
}

impl PaintLayerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the layers of the fragments under `key` into `tree`, and return
    /// the root layer. Keep it to paint the layers and to build again: `tree`
    /// does not store it.
    ///
    /// With `layer`, the root layer of a previous build, only the layers of
    /// the fragments under `key` are built again, after their fragments or
    /// stacking changed, such as after a fragment build or a paint style
    /// change resolved into `pre_paint`. The child layers of the layer `key`
    /// paints in are placed again: layers started under `key` are rebuilt,
    /// and layers started elsewhere in that layer are kept as they are. Pass
    /// the same keys as to
    /// [`PrePaintTreeResolver::resolve`](crate::pre_paint::resolver::PrePaintTreeResolver::resolve).
    /// A fragment moved to another parent needs a key above both its old and
    /// new places.
    ///
    /// Without `layer`, or when `layer` is gone from `tree` or owned by
    /// `key`, `key` is the root fragment: every layer under it is built, and
    /// layers of other fragments, such as those left by a previous root, are
    /// deleted. The root layer has z index 0, whatever `key`'s stacking.
    ///
    /// Layers whose owner still starts one keep their keys. `pre_paint` must
    /// be resolved for the fragments built. Fragments without pre-paint data
    /// start no layer. Returns `None` if `key` is not in `fragments`, or
    /// paints in no layer of `tree` when building from `layer`.
    pub fn build(
        &mut self,
        fragments: &FragmentTree,
        pre_paint: &PrePaintTree,
        tree: &mut PaintLayerTree,
        key: FragmentKey,
        layer: Option<PaintLayerKey>,
    ) -> Option<PaintLayerKey> {
        match layer {
            Some(root) if tree.layers[root].owner != key => {
                self.update(fragments, pre_paint, tree, key)?;
                Some(root)
            }
            _ => self.build_root(fragments, pre_paint, tree, key),
        }
    }

    /// Build every layer under `root`, deleting the layers of other
    /// fragments, and return the root layer.
    fn build_root(
        &mut self,
        fragments: &FragmentTree,
        pre_paint: &PrePaintTree,
        tree: &mut PaintLayerTree,
        root: FragmentKey,
    ) -> Option<PaintLayerKey> {
        fragments.items.get(root)?;
        self.pass = self.pass.wrapping_add(1);

        let root_layer = match tree.layer_of(root) {
            Some(layer) => {
                tree.layers.remove_parent(layer);
                layer
            }
            None => self.insert(tree, root, 0),
        };
        tree.layers[root_layer].z_index = 0;

        self.rebuild(fragments, pre_paint, tree, root_layer, None);

        // Layers of fragments outside `root`, such as previous roots, were
        // not reached.
        self.deleting.clear();
        self.deleting
            .extend(tree.by_owner.values().copied().filter(|&layer| {
                tree.layers
                    .get(layer)
                    .is_some_and(|item| item.pass != self.pass)
            }));
        self.delete_pending(tree);

        Some(root_layer)
    }

    /// Build the layers of the fragments under `key` again, in the layer it
    /// paints in. Returns `None` if `key` is not in `fragments` or paints in
    /// no layer of `tree`.
    fn update(
        &mut self,
        fragments: &FragmentTree,
        pre_paint: &PrePaintTree,
        tree: &mut PaintLayerTree,
        key: FragmentKey,
    ) -> Option<()> {
        fragments.items.get(key)?;

        // A layer `key` starts is placed in its parent layer, which may change
        // with its stacking. The root layer stays.
        let layer = match tree.layer_of(key) {
            Some(layer) => tree.layers.parent(layer).unwrap_or(layer),
            None => tree.enclosing(fragments, key)?,
        };

        self.pass = self.pass.wrapping_add(1);
        self.rebuild(fragments, pre_paint, tree, layer, Some(key));

        Some(())
    }

    /// Place the child layers of `layer` again from its owner's subtree.
    /// Child layers started inside `key`'s subtree, or every child layer when
    /// `key` is `None`, are rebuilt the same way. Detached layers not reached
    /// again are deleted.
    fn rebuild(
        &mut self,
        fragments: &FragmentTree,
        pre_paint: &PrePaintTree,
        tree: &mut PaintLayerTree,
        layer: PaintLayerKey,
        key: Option<FragmentKey>,
    ) {
        let owner = tree.layers[layer].owner;
        let inside = key.is_none_or(|key| key == owner);

        tree.layers[layer].pass = self.pass;
        self.detached.clear();
        self.pending.clear();
        self.detach_children(tree, layer);
        self.push_children(fragments, owner, layer, inside, key);

        while let Some(Visit {
            fragment,
            layer,
            inside,
        }) = self.pending.pop()
        {
            let stacking = pre_paint.fragment(fragment).and_then(|data| data.stacking);

            let Some(z_index) = stacking else {
                self.push_children(fragments, fragment, layer, inside, key);
                continue;
            };

            let child = match tree.layer_of(fragment) {
                Some(child) => child,
                None => self.insert(tree, fragment, z_index),
            };
            let item = &mut tree.layers[child];
            item.z_index = z_index;
            item.pass = self.pass;
            insert_sorted(tree, layer, child);

            // Layers started outside the rebuilt subtree keep their children.
            if inside {
                self.detach_children(tree, child);
                self.push_children(fragments, fragment, child, inside, key);
            }
        }

        self.deleting.clear();
        for &layer in &self.detached {
            if tree
                .layers
                .get(layer)
                .is_some_and(|item| item.pass != self.pass)
            {
                self.deleting.push(layer);
            }
        }
        self.delete_pending(tree);
    }

    /// Insert a parentless layer owned by `owner`.
    fn insert(&self, tree: &mut PaintLayerTree, owner: FragmentKey, z_index: i32) -> PaintLayerKey {
        let layer = tree.layers.insert(PaintLayerItem {
            owner,
            z_index,
            pass: self.pass,
        });
        tree.by_owner.insert(owner, layer);

        layer
    }

    /// Detach the child layers of `layer`, keeping them for reuse.
    fn detach_children(&mut self, tree: &mut PaintLayerTree, layer: PaintLayerKey) {
        while let Some(child) = tree.layers.first_child(layer) {
            tree.layers.remove_parent(child);
            self.detached.push(child);
        }
    }

    /// Queue the children of `fragment` in tree order, painting in `layer`.
    fn push_children(
        &mut self,
        fragments: &FragmentTree,
        fragment: FragmentKey,
        layer: PaintLayerKey,
        inside: bool,
        key: Option<FragmentKey>,
    ) {
        self.pending
            .extend(fragments.items.children(fragment).rev().map(|child| Visit {
                fragment: child,
                layer,
                inside: inside || key == Some(child),
            }));
    }

    /// Delete the queued layers and the layers left under them. Layers are
    /// deleted as leaves, children first, so deleting allocates nothing.
    fn delete_pending(&mut self, tree: &mut PaintLayerTree) {
        while let Some(&layer) = self.deleting.last() {
            if tree.layers.get(layer).is_none() {
                self.deleting.pop();
                continue;
            }

            if let Some(child) = tree.layers.first_child(layer) {
                self.deleting.push(child);
                continue;
            }

            self.deleting.pop();

            if let Some(item) = tree.layers.delete_node(layer)
                && tree.by_owner.get(item.owner) == Some(&layer)
            {
                tree.by_owner.remove(item.owner);
            }
        }
    }
}

/// Attach `child` to `parent` after the last child layer with a z index at
/// most its own. Children attached in tree order then stay in tree order
/// among equal z indices.
fn insert_sorted(tree: &mut PaintLayerTree, parent: PaintLayerKey, child: PaintLayerKey) {
    let z_index = tree.layers[child].z_index;
    let after = tree
        .layers
        .children(parent)
        .rev()
        .find(|&sibling| sibling != child && tree.layers[sibling].z_index <= z_index);

    match after {
        Some(sibling) => tree.layers.after(sibling, child),
        None => tree.layers.prepend(parent, child),
    };
}
