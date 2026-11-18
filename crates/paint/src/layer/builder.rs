use comet_layout::{
    dirty_map::DirtyMap,
    fragment::{FragmentKey, tree::FragmentTree},
    input::{InputFlags, LayoutInputKey, tree::LayoutInputTree},
};

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
    /// Number of the current build, stamped on the layers it reaches.
    pass: u32,
    /// Fragments to place, with the layer they paint in.
    pending: Vec<Visit>,
    /// Layout input items to check for layer changes.
    items: Vec<LayoutInputKey>,
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

    /// Build the layers of the fragments generated under the layout input
    /// item `root` into `tree`, and return the root layer, owned by the box
    /// fragment of `root`. The root fragment always starts the root layer,
    /// with z index 0, whatever its stacking.
    ///
    /// When `tree` already holds the root layer of the root fragment, only
    /// the layers with changes are placed again. Items are visited from
    /// `root`, going into the children of an item whose
    /// [`DirtyEntry::child_bits`](comet_layout::dirty_map::DirtyEntry::child_bits)
    /// in `dirty` have [`InputFlags::NEED_PRE_PAINT`] or
    /// [`InputFlags::NEED_CHILDREN`].
    /// The child layers of the layer enclosing an item marked
    /// [`InputFlags::NEED_CHILDREN`] are placed again, in their new order.
    /// For each item whose
    /// [`DirtyEntry::flags`](comet_layout::dirty_map::DirtyEntry::flags)
    /// have it, the [stacking](crate::pre_paint::FragmentData::stacking) of
    /// each of its fragments is compared with the layer the fragment starts:
    /// when it gains, loses or changes the z index of its layer, the child
    /// layers of the layer it paints in are placed again. So are those of a
    /// layer one of whose child layers lost its owner. Layers started by new
    /// fragments are built, and layers whose owner no longer starts one are
    /// deleted, their child layers moving to the layer they paint in.
    ///
    /// Otherwise, such as on the first build or after a new root, every layer
    /// is built and layers of other fragments are deleted.
    ///
    /// Build from the same `pre_paint` every time, after resolving it for
    /// `fragments`, and before clearing `dirty`. Fragments without
    /// pre-paint data start no layer. Layers whose owner still starts one
    /// keep their keys. Returns `None` if `root` has no box fragment in
    /// `fragments`.
    pub fn build(
        &mut self,
        fragments: &FragmentTree,
        inputs: &LayoutInputTree,
        dirty: &DirtyMap,
        pre_paint: &PrePaintTree,
        tree: &mut PaintLayerTree,
        root: LayoutInputKey,
    ) -> Option<PaintLayerKey> {
        let root_fragment = fragments.box_fragment(root)?;
        self.pass = self.pass.wrapping_add(1);
        self.detached.clear();

        let root_layer = match tree.layer_of(root_fragment) {
            Some(layer) if tree.layers.parent(layer).is_none() => {
                self.update(fragments, inputs, dirty, pre_paint, tree, root, layer);
                layer
            }
            _ => self.build_root(fragments, pre_paint, tree, root_fragment),
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

    /// Place the child layers of the layers with layer changes again.
    #[allow(clippy::too_many_arguments)]
    fn update(
        &mut self,
        fragments: &FragmentTree,
        inputs: &LayoutInputTree,
        dirty: &DirtyMap,
        pre_paint: &PrePaintTree,
        tree: &mut PaintLayerTree,
        root: LayoutInputKey,
        root_layer: PaintLayerKey,
    ) {
        let root_fragment = tree.layers[root_layer].owner;
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
        self.items.clear();
        self.items.push(root);

        while let Some(key) = self.items.pop() {
            if inputs.items.get(key).is_none() {
                continue;
            }
            let entry = dirty.entry(key);

            if entry.flags.contains(InputFlags::NEED_PRE_PAINT) {
                for fragment in fragments.fragments(key) {
                    // The root layer has z index 0, whatever the root's
                    // stacking.
                    if fragment == root_fragment {
                        continue;
                    }

                    let stacking = pre_paint.fragment(fragment).and_then(|data| data.stacking);
                    let current = tree
                        .layer_of(fragment)
                        .map(|layer| tree.layers[layer].z_index);

                    if stacking != current
                        && let Some(layer) = fragments
                            .items
                            .parent(fragment)
                            .and_then(|parent| tree.enclosing(fragments, parent))
                    {
                        self.dirty.push((0, layer));
                    }
                }
            }

            // Children inserted, removed or reordered can reorder the layers
            // they start, which are child layers of the layer enclosing the
            // item's fragments: its own layer if it starts one.
            if entry.flags.contains(InputFlags::NEED_CHILDREN)
                && let Some(layer) = fragments
                    .fragments(key)
                    .next()
                    .and_then(|fragment| tree.enclosing(fragments, fragment))
            {
                self.dirty.push((0, layer));
            }

            // A child list change alone carries no `NEED_PRE_PAINT`.
            if entry
                .child_bits
                .intersects(InputFlags::NEED_PRE_PAINT | InputFlags::NEED_CHILDREN)
            {
                self.items.extend(inputs.items.children(key));
            }
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
