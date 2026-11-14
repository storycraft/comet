use std::mem;

use comet_layout::{
    fragment::{Fragment, FragmentKey, tree::FragmentTree},
    input::tree::LayoutInputTree,
    style::geometry::AutoValue,
};
use comet_tree::ui::{Props, Ui};

use crate::{
    layer::{PaintLayerEntry, PaintLayerItem, PaintLayerKey, tree::PaintLayerTree},
    style::{Opacity, Transform, ZIndex},
};

/// Builds a [`PaintLayerTree`] for a fragment tree. Keep it to reuse its
/// buffers across builds.
#[derive(Default)]
pub struct PaintLayerBuilder {
    /// Fragments to place, with the layer they would paint in.
    pending: Vec<(FragmentKey, PaintLayerKey)>,
    /// Layers to sort the entries of.
    layers: Vec<PaintLayerKey>,
}

impl PaintLayerBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the layers of the fragments under `root` into `tree`, replacing
    /// its previous layers, and return the root layer owned by `root`. Keep
    /// it to paint the layers: `tree` does not store it.
    ///
    /// A box fragment below `root` starts a child layer when its item sets a
    /// non-identity [`Transform`], an [`Opacity`] other than 1, or a
    /// [`ZIndex`] other than auto. An auto z index counts as 0. The root
    /// layer has z index 0, whatever `root` sets.
    ///
    /// `inputs` is the input tree `fragments` were built from, and `ui` the UI
    /// it was built from.
    ///
    /// Returns `None` if a fragment no longer matches the UI, such as when its
    /// source node was deleted after `inputs` and `fragments` were built.
    /// `tree` is then partly built: rebuild the layout and build again.
    ///
    /// # Panics
    ///
    /// Panics if `root` is not in `fragments`.
    pub fn build(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        fragments: &FragmentTree,
        tree: &mut PaintLayerTree,
        root: FragmentKey,
    ) -> Option<PaintLayerKey> {
        tree.clear();

        let root_layer = tree.items.insert(PaintLayerItem {
            owner: root,
            parent: None,
            z_index: 0,
            entries: Vec::new(),
        });

        // Fragments are placed in tree order, so each layer lists its
        // entries in tree order before sorting.
        let pending = &mut self.pending;
        pending.clear();
        pending.push((root, root_layer));

        while let Some((key, parent_layer)) = pending.pop() {
            let fragment = &fragments.items[key];
            let props = ui.props(inputs.items[fragment.source].node)?;

            let layer = match layer_z_index(fragment, props) {
                Some(z_index) if key != root => {
                    let layer = tree.items.insert(PaintLayerItem {
                        owner: key,
                        parent: Some(parent_layer),
                        z_index,
                        entries: Vec::new(),
                    });
                    tree.items[parent_layer]
                        .entries
                        .push(PaintLayerEntry::Layer(layer));

                    layer
                }
                _ => parent_layer,
            };

            tree.items[layer]
                .entries
                .push(PaintLayerEntry::Fragment(key));
            pending.extend(
                fragments
                    .items
                    .children(key)
                    .rev()
                    .map(|child| (child, layer)),
            );
        }

        self.sort_entries(tree);

        Some(root_layer)
    }

    /// Sort the entries of every layer after its owner by z index, keeping
    /// the tree order of equal z indices.
    fn sort_entries(&mut self, tree: &mut PaintLayerTree) {
        self.layers.clear();
        self.layers.extend(tree.items.keys());

        for &key in &self.layers {
            let mut entries = mem::take(&mut tree.items[key].entries);

            // The owner fragment paints first, below its negative z index
            // layers.
            entries[1..].sort_by_key(|entry| match *entry {
                PaintLayerEntry::Fragment(_) => 0,
                PaintLayerEntry::Layer(layer) => tree.items[layer].z_index,
            });

            tree.items[key].entries = entries;
        }
    }
}

/// The z index of the layer a fragment starts, or `None` if it paints in the
/// layer of its parent. Only boxes start layers.
fn layer_z_index(fragment: &Fragment, props: Props<'_>) -> Option<i32> {
    if !fragment.item.is_box() {
        return None;
    }

    let ZIndex(z_index) = props.get_or_default();
    let Transform(transform) = props.get_or_default();
    let Opacity(opacity) = props.get_or_default();

    match z_index {
        AutoValue::Value(z_index) => Some(z_index),
        AutoValue::Auto if transform != Transform::default().0 || opacity != 1.0 => Some(0),
        AutoValue::Auto => None,
    }
}
