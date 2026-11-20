//! Layout and paint state of a window.

use accesskit::{NodeId, TreeId, TreeInfo, TreeUpdate};
use comet_accesskit::{
    A11yTree, A11yTreeBuilder,
    tree::{fragment_key, node_id},
};
use comet_layout::{
    dirty_map::DirtyMap,
    fragment::{FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint,
        geometry::{Size, UnitResolveContext},
    },
};
use comet_paint::{
    hit::PaintHitTester,
    layer::{PaintLayerKey, builder::PaintLayerBuilder, tree::PaintLayerTree},
    painter::CometPainter,
    prepaint::{builder::PrepaintTreeBuilder, tree::PrepaintTree},
};
use comet_reactivity::{PropFlags, UiTree};
use comet_tree::{node::NodeKey, ui::Ui};

/// Font size of the root element, used for `rem`.
const ROOT_FONT_SIZE: f64 = 16.0;

/// Every step is incremental: a layout without changes or a new viewport
/// does little work.
#[derive(Default)]
pub(crate) struct Pipeline {
    /// Layout items of the UI.
    inputs: LayoutInputTree,
    /// Changes taken from the UI and not consumed by the layout yet.
    dirty: DirtyMap,
    input_builder: LayoutInputTreeBuilder,
    fragment_builder: FragmentTreeBuilder,
    fragments: FragmentTree,
    prepaint_builder: PrepaintTreeBuilder,
    prepaint: PrepaintTree,
    layer_builder: PaintLayerBuilder,
    layers: PaintLayerTree,
    hit_tester: PaintHitTester,
    painter: CometPainter,
    /// Root layer of `layers`, from the last layout. `None` until a layout
    /// succeeds, or after one fails.
    layer_root: Option<PaintLayerKey>,
    /// Logical viewport of the last successful layout.
    viewport: Option<Size<f64>>,
    /// Logical viewport the input tree was resolved with, kept when a later
    /// step of the layout fails.
    input_viewport: Option<Size<f64>>,
    /// Logical viewport the pre-paint tree was last resolved with, kept when
    /// a resolve fails, so the next one resolves viewport units again.
    prepaint_viewport: Option<Size<f64>>,
    a11y: Accessibility,
}

impl Pipeline {
    /// Take the changes recorded in `tree`, to be consumed by the next
    /// layout: layout flags are marked on the layout items, and nodes with
    /// accessibility changes are kept to be marked in the accessibility
    /// tree. Nodes not laid out yet have nothing to mark: a new node is laid
    /// out with the children of its parent.
    pub fn take_changes(&mut self, tree: &mut UiTree) {
        for (node, flags) in tree.take_changes() {
            let layout = flags.layout();
            if !layout.is_empty() {
                self.dirty.mark_node(&self.inputs, node, layout);
            }

            if flags.contains(PropFlags::A11Y) && self.a11y.active {
                self.a11y.marks.push(node);
            }
        }
    }

    /// Take the changes of `tree`, then lay them out at `viewport` and place
    /// the layers. On failure, nothing is painted until a later layout
    /// succeeds.
    pub fn layout(&mut self, tree: &mut UiTree, root: NodeKey, viewport: Size<f64>) -> Option<()> {
        self.take_changes(tree);
        let ui = &tree.ui;
        // Cleared until the layout succeeds.
        let previous_viewport = self.viewport.take();
        // Cleared until the layers are placed. The layer tree keeps the root
        // layer, so the layers keep their keys.
        self.layer_root = None;

        // The first layout builds the whole tree. Later ones resolve the
        // marked changes and, after resolving at another viewport, the items
        // whose text styles depend on it.
        if self
            .input_viewport
            .is_some_and(|previous| previous != viewport)
            && let Some(key) = self.inputs.item_for_node(root)
        {
            self.inputs.invalidate_viewport(&mut self.dirty, key);
        }
        let request = BuildRequest::new(root, ROOT_FONT_SIZE, viewport);
        let input_root =
            self.input_builder
                .build(ui, &mut self.inputs, &mut self.dirty, request)?;
        self.input_viewport = Some(viewport);

        let constraint = LayoutConstraint::new_root(viewport, ROOT_FONT_SIZE)
            .with_viewport_changed(previous_viewport.is_some_and(|previous| previous != viewport));
        let Some(fragment_root) = self.fragment_builder.build(
            ui,
            &self.inputs,
            &mut self.dirty,
            input_root,
            &mut self.fragments,
            &constraint,
        ) else {
            self.dirty.clear(&self.inputs, input_root);
            return None;
        };

        // Positions and clips follow the layout, so resolve the fragments of
        // the items it laid out, which it marked, and of the other marked
        // items.
        let resolved = self.resolve_prepaint(ui, &constraint.units, fragment_root);

        // Layers keep their keys, so building from the root again only places
        // the layers whose fragments gained, lost or reordered a layer.
        let layer_root = resolved.and_then(|()| {
            self.layer_builder.build(
                &self.fragments,
                &self.inputs,
                &self.dirty,
                &self.prepaint,
                &mut self.layers,
                input_root,
            )
        });
        // Flags were consumed by the fragment build, the pre-paint resolve,
        // the layer build and the accessibility build. On failure they are
        // kept, so the next layout resolves the items laid out by this one
        // again.
        self.layer_root = Some(layer_root?);
        self.build_accessibility(ui, input_root);
        self.dirty.clear(&self.inputs, input_root);
        self.viewport = Some(viewport);

        Some(())
    }

    /// Resolve the pre-paint tree for the fragments under `root`, with paint
    /// values using viewport units resolved again when `units` has another
    /// viewport than the last successful resolve.
    fn resolve_prepaint(
        &mut self,
        ui: &Ui,
        units: &UnitResolveContext,
        root: FragmentKey,
    ) -> Option<()> {
        let viewport_changed = self
            .prepaint_viewport
            .is_some_and(|previous| previous != units.viewport);

        self.prepaint_builder.build(
            ui,
            &self.inputs,
            &mut self.dirty,
            &self.fragments,
            units,
            &mut self.prepaint,
            root,
            viewport_changed,
        )?;
        self.prepaint_viewport = Some(units.viewport);

        Some(())
    }

    /// Paint the last layout of `tree` into `scene`, covering `physical`.
    /// Does nothing until a layout succeeds.
    pub fn paint(
        &mut self,
        tree: &UiTree,
        physical: kurbo::Rect,
        scale_factor: f64,
        scene: &mut impl anyrender::PaintScene,
    ) {
        let Some(layer_root) = self.layer_root else {
            return;
        };

        self.painter.draw(
            &tree.ui,
            &self.inputs,
            &self.fragments,
            &self.prepaint,
            &self.layers,
            layer_root,
            physical,
            scale_factor,
            scene,
        );
    }

    /// The node under `point`, in logical units, from the last layout.
    pub fn hit(&mut self, point: kurbo::Point) -> Option<NodeKey> {
        let key = self.hit_tester.hit(
            &self.layers,
            &self.fragments,
            &self.prepaint,
            self.layer_root?,
            point,
        )?;
        let source = self.fragments.items.get(key)?.source;

        Some(self.inputs.items.get(source)?.node)
    }
}

/// Accessibility tree of a window, built with the layout while an assistive
/// technology is active.
#[derive(Default)]
pub(crate) struct Accessibility {
    tree: A11yTree,
    builder: A11yTreeBuilder,
    /// Whether the platform adapter is active, so the tree is built.
    active: bool,
    /// Root fragment of the last successful build. `None` until one
    /// succeeds, or after one fails.
    root: Option<FragmentKey>,
    /// Scale factor of the last update, applied to the root node.
    scale_factor: Option<f64>,
    /// Nodes with accessibility changes taken from the UI, marked in the
    /// tree by the next build.
    marks: Vec<NodeKey>,
}

impl Pipeline {
    /// Start or stop building the accessibility tree. Starting builds the
    /// whole tree with the next layout, for the adapter's initial tree.
    pub fn set_accessibility(&mut self, active: bool) {
        let a11y = &mut self.a11y;
        a11y.active = active;
        a11y.tree.clear();
        a11y.root = None;
        a11y.scale_factor = None;
        // A new tree resolves every node.
        a11y.marks.clear();
    }

    /// Build the accessibility tree for the last layout. A failed build
    /// starts over with the next one, which sends the whole tree again.
    fn build_accessibility(&mut self, ui: &Ui, root: LayoutInputKey) {
        let a11y = &mut self.a11y;
        if !a11y.active {
            return;
        }

        // Marked after the layout, on the fragments the nodes have now.
        for node in a11y.marks.drain(..) {
            a11y.tree.mark_node(&self.inputs, &self.fragments, node);
        }

        a11y.root = a11y
            .builder
            .build(
                ui,
                &self.inputs,
                &self.dirty,
                &self.fragments,
                &self.prepaint,
                &mut a11y.tree,
                root,
            )
            .and_then(|()| self.fragments.box_fragment(root));

        if a11y.root.is_none() {
            a11y.tree.clear();
        }
    }

    /// The accessibility changes since the last update, with the root node
    /// scaled from logical units to the physical pixels of `scale_factor`.
    /// `None` while inactive or without changes.
    pub fn take_accessibility_update(&mut self, scale_factor: f64) -> Option<TreeUpdate> {
        let a11y = &mut self.a11y;
        let root = a11y.root.filter(|_| a11y.active)?;
        let root_id = node_id(root);
        let mut nodes = a11y.tree.take_update();

        // The root node carries the scale factor, so a new one updates it.
        if a11y.scale_factor != Some(scale_factor)
            && !nodes.iter().any(|(id, _)| *id == root_id)
            && let Some(node) = a11y.tree.get(root)
        {
            nodes.push((root_id, node.node.clone()));
        }
        a11y.scale_factor = Some(scale_factor);

        if nodes.is_empty() {
            return None;
        }

        for (_, node) in nodes.iter_mut().filter(|(id, _)| *id == root_id) {
            let transform = node
                .transform()
                .map_or(kurbo::Affine::IDENTITY, |transform| {
                    kurbo::Affine::new(transform.as_coeffs())
                });
            let scaled = kurbo::Affine::scale(scale_factor) * transform;
            node.set_transform(accesskit::Affine::new(scaled.as_coeffs()));
        }

        Some(TreeUpdate {
            nodes,
            // Sending it again with every update is allowed.
            tree: Some(TreeInfo::new(root_id)),
            tree_id: TreeId::ROOT,
            // No focus yet.
            focus: root_id,
        })
    }

    /// The source node of an accessibility node, such as the target of an
    /// action request.
    pub fn accessibility_target(&self, id: NodeId) -> Option<NodeKey> {
        let source = self.fragments.items.get(fragment_key(id))?.source;

        Some(self.inputs.items.get(source)?.node)
    }
}
