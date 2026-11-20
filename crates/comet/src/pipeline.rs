//! Layout and paint state of a window.

use comet_layout::{
    fragment::{FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::builder::{BuildRequest, LayoutInputTreeBuilder},
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
use comet_reactivity::UiTree;
use comet_tree::node::NodeKey;

/// Font size of the root element, used for `rem`.
const ROOT_FONT_SIZE: f64 = 16.0;

/// Every step is incremental: a layout without marked changes or a new
/// viewport does little work.
#[derive(Default)]
pub(crate) struct Pipeline {
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
}

impl Pipeline {
    /// Lay out the changes marked in `tree` at `viewport` and place the
    /// layers. On failure, nothing is painted until a later layout succeeds.
    pub fn layout(&mut self, tree: &mut UiTree, root: NodeKey, viewport: Size<f64>) -> Option<()> {
        let UiTree {
            ui, inputs, dirty, ..
        } = tree;
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
            && let Some(key) = inputs.item_for_node(root)
        {
            inputs.invalidate_viewport(dirty, key);
        }
        let request = BuildRequest::new(root, ROOT_FONT_SIZE, viewport);
        let input_root = self.input_builder.build(ui, inputs, dirty, request)?;
        self.input_viewport = Some(viewport);

        let constraint = LayoutConstraint::new_root(viewport, ROOT_FONT_SIZE)
            .with_viewport_changed(previous_viewport.is_some_and(|previous| previous != viewport));
        let Some(fragment_root) = self.fragment_builder.build(
            ui,
            inputs,
            dirty,
            input_root,
            &mut self.fragments,
            &constraint,
        ) else {
            dirty.clear(inputs, input_root);
            return None;
        };

        // Positions and clips follow the layout, so resolve the fragments of
        // the items it laid out, which it marked, and of the other marked
        // items.
        let resolved = self.resolve_prepaint(tree, &constraint.units, fragment_root);
        let UiTree {
            ui: _,
            inputs,
            dirty,
            ..
        } = tree;

        // Layers keep their keys, so building from the root again only places
        // the layers whose fragments gained, lost or reordered a layer.
        let layer_root = resolved.and_then(|()| {
            self.layer_builder.build(
                &self.fragments,
                inputs,
                dirty,
                &self.prepaint,
                &mut self.layers,
                input_root,
            )
        });
        // Flags were consumed by the fragment build, the pre-paint resolve and
        // the layer build. On failure they are kept, so the next layout
        // resolves the items laid out by this one again.
        self.layer_root = Some(layer_root?);
        dirty.clear(inputs, input_root);
        self.viewport = Some(viewport);

        Some(())
    }

    /// Resolve the pre-paint tree for the fragments under `root`, with paint
    /// values using viewport units resolved again when `units` has another
    /// viewport than the last successful resolve.
    fn resolve_prepaint(
        &mut self,
        tree: &mut UiTree,
        units: &UnitResolveContext,
        root: FragmentKey,
    ) -> Option<()> {
        let viewport_changed = self
            .prepaint_viewport
            .is_some_and(|previous| previous != units.viewport);

        self.prepaint_builder.build(
            &tree.ui,
            &tree.inputs,
            &mut tree.dirty,
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
            &tree.inputs,
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
    pub fn hit(&mut self, tree: &UiTree, point: kurbo::Point) -> Option<NodeKey> {
        let key = self.hit_tester.hit(
            &self.layers,
            &self.fragments,
            &self.prepaint,
            self.layer_root?,
            point,
        )?;
        let source = self.fragments.items.get(key)?.source;

        Some(tree.inputs.items.get(source)?.node)
    }
}
