use comet_layout::fragment::{FragmentKey, tree::FragmentTree};

use crate::{
    layer::{PaintLayerItem, PaintLayerKey, tree::PaintLayerTree},
    pre_paint::{LayerFlags, tree::PrePaintTree},
};

/// Builds and updates a [`PaintLayerTree`] for a fragment tree, from the
/// [stacking](crate::pre_paint::FragmentData::stacking) z indices resolved in
/// its [`PrePaintTree`]. Keep it to reuse its buffers: once the trees settle,
/// building again allocates nothing.
#[derive(Default)]
pub struct PaintLayerBuilder {
    /// Number of the current build, stamped on the layers it reaches.
    pass: u32,
    /// Fragments to place, with the layer they paint in.
    pending: Vec<Visit>,
    /// Fragments to check for layer flags, with the layer of their nearest
    /// ancestor starting one.
    changes: Vec<Visit>,
    /// Layers to place the child layers of again, with their depths.
    dirty: Vec<(usize, PaintLayerKey)>,
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
}

impl PaintLayerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the layers of the fragments under `root` into `tree`, and
    /// return the root layer, owned by `root`. The root layer has z index 0,
    /// whatever `root`'s stacking.
    ///
    /// When `tree` already holds the root layer of `root`, only the layers
    /// with changes are placed again, found from the root with the
    /// [layer flags](crate::pre_paint::LayerFlags) of `pre_paint`: a layer's
    /// child layers are placed again when a fragment painting in it is
    /// flagged [`NEED_LAYER`](LayerFlags::NEED_LAYER) or
    /// [`NEED_NO_LAYER`](LayerFlags::NEED_NO_LAYER), or when one of its child
    /// layers lost its owner. Layers started by new fragments are built, and
    /// layers whose owner no longer starts one are deleted, their child
    /// layers moving to the layer they paint in. Subtrees without
    /// [`CHILD_LAYER_UPDATE`](LayerFlags::CHILD_LAYER_UPDATE) are skipped.
    ///
    /// Otherwise, such as on the first build or after a new root, every layer
    /// is built and layers of other fragments are deleted. After
    /// [clearing](PrePaintTree::clear) `pre_paint`, also
    /// [clear](PaintLayerTree::clear) `tree`: layer flags do not cover what
    /// changed before.
    ///
    /// Build from the same `pre_paint` every time, after resolving it for
    /// `fragments`. Layer flags are not cleared; call
    /// [`PrePaintTree::clear_layer_flags`] after building. Fragments without
    /// pre-paint data start no layer.
    /// Layers whose owner still starts one keep their keys. Returns `None`
    /// if `root` is not in `fragments`.
    pub fn build(
        &mut self,
        fragments: &FragmentTree,
        pre_paint: &PrePaintTree,
        tree: &mut PaintLayerTree,
        root: FragmentKey,
    ) -> Option<PaintLayerKey> {
        fragments.items.get(root)?;
        self.pass = self.pass.wrapping_add(1);
        self.detached.clear();

        let root_layer = match tree.layer_of(root) {
            Some(layer) if tree.layers.parent(layer).is_none() => {
                self.update(fragments, pre_paint, tree, root, layer);
                layer
            }
            _ => self.build_root(fragments, pre_paint, tree, root),
        };

        self.delete_detached(tree);

        Some(root_layer)
    }

    /// Build every layer under `root`, deleting the layers of other
    /// fragments, and return the root layer.
    fn build_root(
        &mut self,
        fragments: &FragmentTree,
        pre_paint: &PrePaintTree,
        tree: &mut PaintLayerTree,
        root: FragmentKey,
    ) -> PaintLayerKey {
        let root_layer = match tree.layer_of(root) {
            Some(layer) => {
                tree.layers.remove_parent(layer);
                layer
            }
            None => self.insert(tree, root, 0),
        };
        tree.layers[root_layer].z_index = 0;

        self.rebuild(fragments, pre_paint, tree, root_layer, true);

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

        root_layer
    }

    /// Place the child layers of the layers with flagged fragments again.
    fn update(
        &mut self,
        fragments: &FragmentTree,
        pre_paint: &PrePaintTree,
        tree: &mut PaintLayerTree,
        root: FragmentKey,
        root_layer: PaintLayerKey,
    ) {
        self.dirty.clear();

        // Layers whose owners were deleted are not reached from the
        // fragments, so the layers they paint in are placed again.
        for (owner, &layer) in &tree.by_owner {
            if fragments.items.get(owner).is_none()
                && let Some(parent) = tree.layers.parent(layer)
            {
                self.dirty.push((0, parent));
            }
        }

        // A fragment gaining, losing or reordering its layer changes the
        // child layers of the layer it paints in, found from the layers its
        // ancestors started before this build.
        self.changes.clear();
        self.changes.push(Visit {
            fragment: root,
            layer: root_layer,
        });

        while let Some(Visit { fragment, layer }) = self.changes.pop() {
            let flags = pre_paint.layer_flags(fragment);

            // The root layer has z index 0, whatever the root's stacking.
            if fragment != root
                && flags.intersects(LayerFlags::NEED_LAYER | LayerFlags::NEED_NO_LAYER)
            {
                self.dirty.push((0, layer));
            }

            if !flags.contains(LayerFlags::CHILD_LAYER_UPDATE) {
                continue;
            }

            let layer = tree.layer_of(fragment).unwrap_or(layer);
            self.changes
                .extend(fragments.items.children(fragment).map(|child| Visit {
                    fragment: child,
                    layer,
                }));
        }

        for (depth, layer) in &mut self.dirty {
            *depth = std::iter::successors(tree.layers.parent(*layer), |&layer| {
                tree.layers.parent(layer)
            })
            .count();
        }

        // Deeper layers first: placing the child layers of a layer again
        // can take grandchildren from a child layer losing its owner, which
        // placing that child layer later would take back.
        self.dirty
            .sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        self.dirty.dedup();

        for index in 0..self.dirty.len() {
            let layer = self.dirty[index].1;
            let Some(item) = tree.layers.get(layer) else {
                continue;
            };

            // A layer whose owner was deleted or no longer starts one is
            // deleted by the layer it paints in, which is placed again.
            let owner = item.owner;
            let starts = layer == root_layer
                || pre_paint
                    .fragment(owner)
                    .is_some_and(|data| data.stacking.is_some());

            if fragments.items.get(owner).is_none() || !starts {
                continue;
            }

            self.rebuild(fragments, pre_paint, tree, layer, false);
        }
    }

    /// Place the child layers of `layer` again from its owner's subtree.
    /// Layers new in this build are built the same way, and so is every
    /// other child layer when `deep`; other child layers keep their
    /// children. Detached child layers are deleted at the end of the build
    /// unless they are reached again.
    fn rebuild(
        &mut self,
        fragments: &FragmentTree,
        pre_paint: &PrePaintTree,
        tree: &mut PaintLayerTree,
        layer: PaintLayerKey,
        deep: bool,
    ) {
        let owner = tree.layers[layer].owner;

        tree.layers[layer].pass = self.pass;
        self.pending.clear();
        self.detach_children(tree, layer);
        self.push_children(fragments, owner, layer);

        while let Some(Visit { fragment, layer }) = self.pending.pop() {
            let stacking = pre_paint.fragment(fragment).and_then(|data| data.stacking);

            let Some(z_index) = stacking else {
                self.push_children(fragments, fragment, layer);
                continue;
            };

            let (child, created) = match tree.layer_of(fragment) {
                Some(child) => (child, false),
                None => (self.insert(tree, fragment, z_index), true),
            };
            let item = &mut tree.layers[child];
            item.z_index = z_index;
            item.pass = self.pass;
            insert_sorted(tree, layer, child);

            if deep || created {
                self.detach_children(tree, child);
                self.push_children(fragments, fragment, child);
            }
        }
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
    ) {
        self.pending
            .extend(fragments.items.children(fragment).rev().map(|child| Visit {
                fragment: child,
                layer,
            }));
    }

    /// Delete the detached layers this build did not reach again.
    fn delete_detached(&mut self, tree: &mut PaintLayerTree) {
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
