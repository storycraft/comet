use std::sync::Arc;

use anyrender::filters::FilterEffect;
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::geometry::AutoValue,
};
use comet_tree::ui::{Props, Ui};

use crate::{
    layer::{PaintLayerItem, PaintLayerKey, tree::PaintLayerTree},
    style::{BackdropFilter, Filter, Opacity, Transform, ZIndex},
};

/// Builds and updates a [`PaintLayerTree`] for a fragment tree, from the
/// layer styles of its items. Keep it to reuse its buffers: once the trees
/// settle, building again allocates nothing but the filters it resolves.
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

/// What building reads from.
#[derive(Clone, Copy)]
struct Sources<'a> {
    ui: &'a Ui,
    inputs: &'a LayoutInputTree,
    fragments: &'a FragmentTree,
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
    /// with z index 0, whatever its styles.
    ///
    /// A box fragment starts a layer when its item sets a [`Transform`], an
    /// [`Opacity`], a [`Filter`] or a [`BackdropFilter`], whatever its value,
    /// or a [`ZIndex`] other than auto. An auto z index counts as 0. Each
    /// layer resolves its opacity and filters from its owner's item.
    ///
    /// When `tree` already holds the root layer, only the layers affected by
    /// changes marked in `dirty` are updated. Layers whose owner still starts
    /// one keep their keys. Items with [`DirtyFlags::NEED_LAYER`], which
    /// [`DirtyFlags::NEED_CHILDREN`] includes, are checked for fragments
    /// gaining, losing or reordering a layer, and the layers they own resolve
    /// their data again. It is marked when a layer style changes, and by the
    /// [pre-paint build](crate::prepaint::builder::PrepaintTreeBuilder::build)
    /// for fragments whose data changed.
    ///
    /// `inputs` is the input tree `fragments` were built from. Build before
    /// clearing `dirty`. Returns `None` if `root` has no box fragment in
    /// `fragments`.
    pub fn build(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        dirty: &DirtyMap,
        fragments: &FragmentTree,
        tree: &mut PaintLayerTree,
        root: LayoutInputKey,
    ) -> Option<PaintLayerKey> {
        let sources = Sources {
            ui,
            inputs,
            fragments,
        };
        let root_fragment = fragments.box_fragment(root)?;
        self.pass = self.pass.wrapping_add(1);
        self.detached.clear();

        let root_layer = match tree.layer_of(root_fragment) {
            Some(layer) if tree.layers.parent(layer).is_none() => {
                self.update(sources, dirty, tree, root, layer);
                layer
            }
            _ => self.build_root(sources, tree, root_fragment),
        };

        self.delete_detached(tree);

        Some(root_layer)
    }

    /// Build every layer under `root`, deleting the layers of other
    /// fragments, and return the root layer.
    fn build_root(
        &mut self,
        sources: Sources<'_>,
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
        let item = &mut tree.layers[root_layer];
        item.z_index = 0;
        sources.resolve(item);

        self.rebuild(sources, tree, root_layer, true);

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

    /// Resolve the data of the layers owned by marked items again, and place
    /// the child layers of the layers with layer changes again.
    fn update(
        &mut self,
        sources: Sources<'_>,
        dirty: &DirtyMap,
        tree: &mut PaintLayerTree,
        root: LayoutInputKey,
        root_layer: PaintLayerKey,
    ) {
        let Sources {
            inputs, fragments, ..
        } = sources;
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

        // Layers are found from the layers the ancestors started before this
        // build.
        self.items.clear();
        self.items.push(root);

        while let Some(key) = self.items.pop() {
            if inputs.items.get(key).is_none() {
                continue;
            }
            let entry = dirty.entry(key);

            if entry.flags.contains(DirtyFlags::NEED_LAYER) {
                for fragment in fragments.fragments(key) {
                    let current = tree.layer_of(fragment);

                    // Layer data follows the owner's styles and its layout.
                    if let Some(item) = current.and_then(|layer| tree.layers.get_mut(layer)) {
                        sources.resolve(item);
                    }

                    // `NEED_CHILDREN` includes `NEED_LAYER`, so the stacking
                    // of each fragment is compared with the layer it starts.
                    // The root layer has z index 0, whatever the root's
                    // styles.
                    if fragment == root_fragment {
                        continue;
                    }

                    let stacking = sources.stacking(fragment);
                    let current = current.map(|layer| tree.layers[layer].z_index);

                    // A fragment gaining, losing or reordering its layer
                    // changes the child layers of the layer it paints in.
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
            if entry.flags.contains(DirtyFlags::NEED_CHILDREN)
                && let Some(layer) = fragments
                    .fragments(key)
                    .next()
                    .and_then(|fragment| tree.enclosing(fragments, fragment))
            {
                self.dirty.push((0, layer));
            }

            if entry.child_bits.contains(DirtyFlags::NEED_LAYER) {
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
            let starts = layer == root_layer || sources.stacking(owner).is_some();

            if fragments.items.get(owner).is_none() || !starts {
                continue;
            }

            self.rebuild(sources, tree, layer, false);
        }
    }

    /// Place the child layers of `layer` again from its owner's subtree.
    /// Layers new in this build are built the same way, and so is every
    /// other child layer when `deep`; other child layers keep their
    /// children. Layers built resolve their data. Detached child layers are
    /// deleted at the end of the build unless they are reached again.
    fn rebuild(
        &mut self,
        sources: Sources<'_>,
        tree: &mut PaintLayerTree,
        layer: PaintLayerKey,
        deep: bool,
    ) {
        let fragments = sources.fragments;
        let owner = tree.layers[layer].owner;

        tree.layers[layer].pass = self.pass;
        self.pending.clear();
        self.detach_children(tree, layer);
        self.push_children(fragments, owner, layer);

        while let Some(Visit { fragment, layer }) = self.pending.pop() {
            let Some(z_index) = sources.stacking(fragment) else {
                self.push_children(fragments, fragment, layer);
                continue;
            };

            let (child, created) = match tree.layer_of(fragment) {
                Some(child) => (child, false),
                None => (self.insert(tree, fragment, z_index), true),
            };
            let build = deep || created;
            let item = &mut tree.layers[child];
            item.z_index = z_index;
            item.pass = self.pass;

            if build {
                sources.resolve(item);
            }

            insert_sorted(tree, layer, child);

            if build {
                self.detach_children(tree, child);
                self.push_children(fragments, fragment, child);
            }
        }
    }

    /// Insert a parentless layer owned by `owner`, with its data unresolved.
    fn insert(&self, tree: &mut PaintLayerTree, owner: FragmentKey, z_index: i32) -> PaintLayerKey {
        let layer = tree.layers.insert(PaintLayerItem {
            owner,
            z_index,
            opacity: 1.0,
            filter: None,
            backdrop_filter: None,
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

impl Sources<'_> {
    /// The props of the node of a layout input item.
    fn props(&self, key: LayoutInputKey) -> Option<Props<'_>> {
        self.ui.props(self.inputs.items.get(key)?.node)
    }

    /// The z index of the layer a fragment starts, or `None` if it paints in
    /// the layer of its parent. Only boxes start layers.
    fn stacking(&self, key: FragmentKey) -> Option<i32> {
        let fragment = self.fragments.items.get(key)?;

        if !fragment.item.is_box() {
            return None;
        }

        stacking(self.props(fragment.source)?)
    }

    /// Resolve the opacity and filters of a layer from its owner's item.
    fn resolve(&self, item: &mut PaintLayerItem) {
        let fragment = self.fragments.items.get(item.owner);
        let props = fragment.and_then(|fragment| self.props(fragment.source));

        item.opacity = props.map_or(1.0, |props| {
            let Opacity(opacity) = props.get_or_default();
            opacity.clamp(0.0, 1.0) as f32
        });
        item.filter = fragment.zip(props).and_then(|(fragment, props)| {
            let effects = props.get::<Filter>()?;
            Some(filter(&effects.0, fragment.layout.overflow))
        });
        item.backdrop_filter = fragment.zip(props).and_then(|(fragment, props)| {
            let effects = props.get::<BackdropFilter>()?;
            Some(filter(&effects.0, fragment.layout.border_box()))
        });
    }
}

/// The z index of the layer a box with `props` starts, or `None` if it paints
/// in the layer of its parent. Setting a layer style starts a layer, whatever
/// its value. An auto z index counts as 0.
fn stacking(props: Props<'_>) -> Option<i32> {
    if let AutoValue::Value(z_index) = props.get_or_default::<ZIndex>().0 {
        return Some(z_index);
    }

    let starts = props.get::<Transform>().is_some()
        || props.get::<Opacity>().is_some()
        || props.get::<Filter>().is_some()
        || props.get::<BackdropFilter>().is_some();

    starts.then_some(0)
}

/// A filter applying `effects` in order to content within `bounds`, its
/// region grown by the expansion of each effect.
fn filter(effects: &[FilterEffect], bounds: kurbo::Rect) -> Arc<anyrender::Filter> {
    Arc::new(anyrender::Filter::linear_list(
        effects.iter().cloned(),
        bounds,
    ))
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
