use comet_tree::{node::NodeKey, ui::Ui};

use super::{FragmentKey, emit, reconcile::Reconciler, snapshot::Snapshot, tree::FragmentTree};
use crate::{
    style::LayoutConstraint,
    taffy::{TaffyLayout, conv},
    tree::{LayoutItemKey, LayoutTree},
};

/// Reconciles a layout subtree into stable fragment slots. Unchanged inputs skip
/// shaping and layout; changed inputs reuse matching box, line, and run slots.
#[derive(Default)]
pub struct FragmentBuilder {
    pub fonts: parley::FontContext,
    parley: parley::LayoutContext<Option<NodeKey>>,
    reconciliation: Reconciler,
    desired: emit::Desired,
}

impl FragmentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call after updating `LayoutTree`. `root` identifies an independent layout root.
    /// Build new fragments with `None`, or update the supplied root with `Some(key)`.
    /// The target must belong to this layout root; descendant targets are not
    /// resolved to an owning root. A missing layout root deletes the target.
    pub fn build(
        &mut self,
        constraint: &LayoutConstraint,
        ui: &Ui,
        layout: &LayoutTree,
        root: LayoutItemKey,
        fragments: &mut FragmentTree,
        target: Option<FragmentKey>,
    ) -> Option<FragmentKey> {
        let old_root = target.filter(|&key| fragments.items.get(key).is_some());
        if layout.items.get(root).is_none() {
            if let Some(key) = old_root {
                fragments.remove_root(key);
            }

            return None;
        }

        let snapshot = Snapshot::capture(constraint, ui, layout, root, fragments);

        if old_root.is_some() && fragments.snapshots.get(root) == Some(&snapshot) {
            return old_root;
        }

        let nodes = snapshot.prepare(layout, root, fragments);
        let mut engine = TaffyLayout {
            constraint: constraint.clone(),
            ui,
            tree: layout,
            fragments,
            fonts: &mut self.fonts,
            parley: &mut self.parley,
            nodes,
        };

        engine.compute(
            root,
            conv::to_size(constraint.available_size, |v| {
                taffy::AvailableSpace::Definite(v as f32)
            }),
        );

        emit::emit_box(&engine, root, None, None, &mut self.desired);
        self.desired.finish();
        engine.fragments.layout_cache = engine.nodes;

        let fragments = engine.fragments;
        let result = self
            .reconciliation
            .apply(fragments, old_root, &mut self.desired, &snapshot);
        fragments.snapshots.insert(root, snapshot);
        fragments
            .text_styles
            .retain(|node, _| ui.kind(*node).is_some());

        Some(result)
    }
}
