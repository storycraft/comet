use accesskit::{NodeId, Role as AxRole};
use comet_accesskit::{
    AxKey, AxNodeKind, ax_key, builder::AxTreeBuilder, node_id, style::Role, tree::AxTree,
};
use comet_content::manager::ContentManager;
use comet_geometry::{AutoValue, Point, Rect, Size, StyleUnit};
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{BoxFragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint,
        bx::{self, DisplayOuter, Padding},
    },
};
use comet_paint::{
    layer::{builder::PaintLayerBuilder, tree::PaintLayerTree},
    prepaint::{builder::PrepaintTreeBuilder, tree::PrepaintTree},
    style::{ScrollTop, Transform},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use kurbo::Affine;
use taffy::Overflow;

const VIEWPORT: Size<f64> = Size::new(400.0, 300.0);

/// The layout steps and the accessibility tree of one UI.
#[derive(Default)]
struct Harness {
    ui: Ui,
    inputs: LayoutInputTree,
    dirty: DirtyMap,
    fragments: FragmentTree,
    prepaint: PrepaintTree,
    layers: PaintLayerTree,
    a11y: AxTree,
    input_builder: LayoutInputTreeBuilder,
    fragment_builder: FragmentTreeBuilder,
    prepaint_builder: PrepaintTreeBuilder,
    layer_builder: PaintLayerBuilder,
    a11y_builder: AxTreeBuilder,
    /// Root item of the last build.
    input_root: Option<LayoutInputKey>,
}

impl Harness {
    /// Lay out the marked changes under `root`, update the accessibility
    /// tree and take its update.
    fn build(&mut self, root: NodeKey) -> Vec<(NodeId, accesskit::Node)> {
        self.layout(root);
        self.a11y.take_update()
    }

    /// Lay out the marked changes under `root` and update the accessibility
    /// tree, keeping its changes.
    fn layout(&mut self, root: NodeKey) {
        let Self {
            ui,
            inputs,
            dirty,
            fragments,
            prepaint,
            layers,
            a11y,
            ..
        } = self;
        let input_root = self
            .input_builder
            .build(ui, inputs, dirty, BuildRequest::new(root, 16.0, VIEWPORT))
            .expect("input tree");
        let constraint = LayoutConstraint::new_root(VIEWPORT, 16.0, 1.0);
        let fragment_root = self
            .fragment_builder
            .build(
                ui,
                inputs,
                &ContentManager::new(),
                dirty,
                input_root,
                fragments,
                &constraint,
            )
            .expect("fragment tree");
        self.prepaint_builder
            .build(
                ui,
                inputs,
                dirty,
                fragments,
                &constraint.units,
                prepaint,
                fragment_root,
                false,
            )
            .expect("pre-paint tree");
        self.layer_builder
            .build(
                ui,
                inputs,
                dirty,
                fragments,
                &constraint.units,
                layers,
                input_root,
                false,
            )
            .expect("paint layers");
        self.a11y_builder
            .build(
                ui, inputs, dirty, fragments, prepaint, layers, a11y, input_root,
            )
            .expect("accessibility tree");
        dirty.clear(inputs, input_root);
        self.input_root = Some(input_root);
    }

    /// The root node of the last build.
    fn root(&self) -> AxKey {
        self.a11y.item_node(self.input_root.unwrap()).unwrap()
    }

    /// The node of a UI node.
    fn key(&self, node: NodeKey) -> AxKey {
        self.a11y
            .item_node(self.inputs.item_for_node(node).unwrap())
            .unwrap()
    }

    /// The box fragment of a UI node.
    fn box_fragment(&self, node: NodeKey) -> BoxFragmentKey {
        self.fragments
            .box_fragment(self.inputs.item_for_node(node).unwrap())
            .unwrap()
    }

    fn mark(&mut self, node: NodeKey, flags: DirtyFlags) {
        self.dirty.mark_node(&self.inputs, node, flags);
    }

    fn node(&self, key: AxKey) -> &accesskit::Node {
        &self.a11y.get(key).expect("node").node
    }

    /// The text run nodes of a text node.
    fn runs(&self, node: NodeKey) -> Vec<AxKey> {
        self.a11y
            .nodes()
            .children(self.key(node))
            .filter(|&key| self.a11y.get(key).unwrap().kind == AxNodeKind::TextRun)
            .collect()
    }

    /// Ids of the updated nodes.
    fn ids(update: &[(NodeId, accesskit::Node)]) -> Vec<NodeId> {
        let mut ids: Vec<_> = update.iter().map(|(id, _)| *id).collect();
        ids.sort();
        ids
    }

    /// Values of the nodes in the accessibility tree, in tree order.
    fn texts(&self) -> String {
        let mut texts = String::new();
        let mut pending = vec![node_id(self.root())];
        while let Some(id) = pending.pop() {
            let node = self.node(ax_key(id));
            texts.extend(node.value());
            pending.extend(node.children().iter().rev());
        }
        texts
    }

    /// Check that every node is as a full build would resolve it, and that
    /// the tree of nodes mirrors their children.
    fn assert_consistent(&self) {
        let mut fresh = AxTree::new();
        AxTreeBuilder::new()
            .build(
                &self.ui,
                &self.inputs,
                &DirtyMap::new(),
                &self.fragments,
                &self.prepaint,
                &self.layers,
                &mut fresh,
                self.input_root.unwrap(),
            )
            .expect("full build");

        let mut pending = vec![(
            self.root(),
            fresh.item_node(self.input_root.unwrap()).unwrap(),
        )];
        while let Some((key, expected_key)) = pending.pop() {
            let actual = self.node(key);
            let expected = &fresh.get(expected_key).unwrap().node;
            // Ids differ between the trees.
            assert_eq!(strip_ids(actual), strip_ids(expected), "node {key:?}");

            let children: Vec<_> = self.a11y.nodes().children(key).collect();
            assert_eq!(
                actual.children(),
                children.iter().copied().map(node_id).collect::<Vec<_>>()
            );
            let expected_children: Vec<_> = fresh.nodes().children(expected_key).collect();
            assert_eq!(
                children.len(),
                expected_children.len(),
                "children of {key:?}"
            );
            pending.extend(children.into_iter().zip(expected_children));
        }

        // Every node is reachable from the root.
        assert_eq!(
            self.a11y.nodes().len(),
            self.a11y.nodes().descendants(self.root()).count()
        );
    }
}

/// A node without the ids it refers to.
fn strip_ids(node: &accesskit::Node) -> accesskit::Node {
    let mut node = node.clone();
    node.clear_children();
    node.clear_previous_on_line();
    node.clear_next_on_line();
    node
}

/// A box with a padded box holding a text.
fn sample(ui: &mut Ui) -> (NodeKey, NodeKey, NodeKey) {
    let root = ui.create(Node::Box, (Role(AxRole::Main),));
    let card = ui.create(
        Node::Box,
        (
            Padding(Rect::new_all(StyleUnit::Px(8.0))),
            Transform(Affine::translate((5.0, 0.0))),
        ),
    );
    let text = ui.create(Node::Text, (Text("Hello comet".into()),));
    ui.append(root, card);
    ui.append(card, text);

    (root, card, text)
}

fn transform_of(node: &accesskit::Node) -> Affine {
    node.transform().map_or(Affine::IDENTITY, |transform| {
        Affine::new(transform.as_coeffs())
    })
}

fn assert_close(actual: Affine, expected: Affine) {
    for (actual, expected) in actual.as_coeffs().iter().zip(expected.as_coeffs()) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }
}

#[test]
fn follows_the_ui_tree() {
    let mut h = Harness::default();
    let (root, card, text) = sample(&mut h.ui);
    let update = h.build(root);

    // One node per item, and one per glyph run, all sent with the first
    // update.
    let runs = h.runs(text);
    assert!(!runs.is_empty());
    let mut expected: Vec<_> = [h.root(), h.key(card), h.key(text)]
        .into_iter()
        .chain(runs.iter().copied())
        .map(node_id)
        .collect();
    expected.sort();
    assert_eq!(Harness::ids(&update), expected);

    let root_node = h.node(h.root());
    assert_eq!(root_node.role(), AxRole::Main);
    assert_eq!(root_node.children(), [node_id(h.key(card))]);

    // The card is placed by its box's transform, with its border box as
    // bounds.
    let card_fragment = h.box_fragment(card);
    let card_node = h.node(h.key(card));
    assert_eq!(card_node.role(), AxRole::Group);
    assert_eq!(
        transform_of(card_node),
        h.layers.transform(&h.fragments, card_fragment).unwrap()
    );
    let size = h.fragments.boxes[card_fragment].layout.geometry.size;
    assert_eq!(
        card_node.bounds(),
        Some(accesskit::Rect::new(0.0, 0.0, size.width, size.height))
    );
    assert_eq!(card_node.children(), [node_id(h.key(text))]);

    // The text is a label in the card's space, covering its runs, which it
    // holds.
    let text_node = h.node(h.key(text));
    assert_eq!(text_node.role(), AxRole::Label);
    assert!(text_node.transform().is_none());
    assert!(text_node.bounds().is_some());
    assert_eq!(
        text_node.children(),
        runs.iter().copied().map(node_id).collect::<Vec<_>>()
    );
    let item = h.inputs.item_for_node(text).unwrap();
    let glyph_runs: Vec<_> = h.fragments.fragments(&h.inputs, item).collect();
    assert_eq!(glyph_runs.len(), runs.len());
    for (&run, &glyph_run) in runs.iter().zip(&glyph_runs) {
        let node = h.node(run);
        assert_eq!(node.role(), AxRole::TextRun);
        // Placed at its location in the card's inline layout.
        let location = h.fragments.geometry(glyph_run).unwrap().location;
        assert_close(
            transform_of(node),
            Affine::translate((location.x, location.y)),
        );
    }
    assert_eq!(h.texts(), "Hello comet");
}

#[test]
fn nodes_follow_the_order_of_the_ui() {
    let mut h = Harness::default();
    let ui = &mut h.ui;
    let root = ui.create(Node::Box, ());
    let head = ui.create(Node::Text, (Text("head ".into()),));
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let inner = ui.create(Node::Text, (Text("inner ".into()),));
    // A block splits the span: its box is not inside the span's fragments.
    let block = ui.create(Node::Box, ());
    let body = ui.create(Node::Text, (Text("block ".into()),));
    let tail = ui.create(Node::Text, (Text("tail".into()),));
    ui.append(root, head);
    ui.append(root, span);
    ui.append(span, inner);
    ui.append(span, block);
    ui.append(block, body);
    ui.append(root, tail);
    h.build(root);

    assert_eq!(h.texts(), "head inner block tail");
    let ids = |keys: &[AxKey]| keys.iter().copied().map(node_id).collect::<Vec<_>>();
    assert_eq!(
        h.node(h.root()).children(),
        ids(&[h.key(head), h.key(span), h.key(tail)])
    );
    assert_eq!(
        h.node(h.key(span)).children(),
        ids(&[h.key(inner), h.key(block)])
    );

    // The span is in the root's space, so the block is placed into the
    // root's box.
    let root_box = h.box_fragment(root);
    let block_box = h.box_fragment(block);
    let to_root = core::iter::successors(Some(block_box), |&key| h.fragments.boxes.parent(key))
        .take_while(|&key| key != root_box)
        .fold(Affine::IDENTITY, |transform, key| {
            h.layers.transform(&h.fragments, key).unwrap() * transform
        });
    assert_close(transform_of(h.node(h.key(block))), to_root);
    h.assert_consistent();

    // New text in the span lays out its inline content again, and the
    // nodes stay as a full build resolves them.
    h.ui.set_props(inner, (Text("changed ".into()),));
    h.mark(inner, DirtyFlags::NEED_FULL_LAYOUT);
    h.build(root);
    assert_eq!(h.texts(), "head changed block tail");
    h.assert_consistent();
}

#[test]
fn text_runs_carry_their_characters() {
    let mut h = Harness::default();
    let (root, _, text) = sample(&mut h.ui);
    let update = h.build(root);

    let runs = h.runs(text);
    for (index, &run) in runs.iter().enumerate() {
        let node = h.node(run);
        let value = node.value().unwrap();
        let lengths = node.character_lengths();
        assert!(node.label().is_none());
        assert_eq!(
            lengths
                .iter()
                .map(|&length| usize::from(length))
                .sum::<usize>(),
            value.len()
        );
        assert_eq!(node.character_positions().unwrap().len(), lengths.len());
        assert_eq!(node.character_widths().unwrap().len(), lengths.len());
        assert_eq!(
            node.text_direction(),
            Some(accesskit::TextDirection::LeftToRight)
        );
        // One line: the runs follow each other.
        assert_eq!(
            node.next_on_line(),
            runs.get(index + 1).copied().map(node_id)
        );
    }

    // The text's label exposes the text of its runs, as platform adapters
    // read it.
    let root_id = node_id(h.root());
    let tree = accesskit_consumer::Tree::new(
        accesskit::TreeUpdate {
            nodes: update,
            tree: Some(accesskit::TreeInfo::new(root_id)),
            tree_id: accesskit::TreeId::ROOT,
            focus: root_id,
        },
        false,
    );
    let text_id = node_id(h.key(text));
    // Under the card.
    let card = tree.state().root().children().next().unwrap();
    let label = card
        .children()
        .find(|node| node.locate().0 == text_id)
        .unwrap();
    assert_eq!(label.value().as_deref(), Some("Hello comet"));
    // Adapters name a label with its value.
    assert!(label.label_comes_from_value());
}

#[test]
fn roles_default_by_node_kind() {
    let mut h = Harness::default();
    let (root, card, text) = sample(&mut h.ui);
    let other = h.ui.create(Node::Text, (Text("other".into()),));
    h.ui.append(root, other);
    // A box of blocks, and an inline element without a box of its own.
    let wrapper = h.ui.create(Node::Box, ());
    let block = h.ui.create(Node::Box, ());
    h.ui.append(root, wrapper);
    h.ui.append(wrapper, block);
    // A box whose inline content is split by a block.
    let split = h.ui.create(Node::Box, ());
    let split_text = h.ui.create(Node::Text, (Text("split ".into()),));
    let split_block = h.ui.create(Node::Box, ());
    h.ui.append(root, split);
    h.ui.append(split, split_text);
    h.ui.append(split, split_block);
    let span = h.ui.create(Node::Box, (DisplayOuter::Inline,));
    let spanned = h.ui.create(Node::Text, (Text(" span".into()),));
    h.ui.append(card, span);
    h.ui.append(span, spanned);
    // Text nodes have no role of their own: one set on them is ignored.
    h.ui.set_props(text, (Role(AxRole::Heading),));
    h.build(root);

    // The card lays out inline content.
    assert_eq!(h.node(h.key(card)).role(), AxRole::Group);
    assert_eq!(h.node(h.key(wrapper)).role(), AxRole::GenericContainer);
    assert_eq!(h.node(h.key(block)).role(), AxRole::GenericContainer);
    assert_eq!(h.node(h.key(span)).role(), AxRole::GenericContainer);
    assert_eq!(h.node(h.key(split)).role(), AxRole::GenericContainer);
    assert_eq!(h.node(h.key(text)).role(), AxRole::Label);
    assert_eq!(h.node(h.key(other)).role(), AxRole::Label);
    for run in h.runs(text) {
        assert_eq!(h.node(run).role(), AxRole::TextRun);
    }
}

#[test]
fn texts_with_boxes_hold_their_runs() {
    let mut h = Harness::default();
    let root = h.ui.create(Node::Text, (Text("Boxed text".into()),));
    h.build(root);

    // The text's box is its node, holding its glyph runs, which it is not
    // linked with.
    let root_node = h.node(h.root());
    assert_eq!(root_node.role(), AxRole::Label);
    assert!(!h.runs(root).is_empty());
    assert_eq!(root_node.children().len(), h.runs(root).len());
    assert_eq!(h.texts(), "Boxed text");
    h.assert_consistent();
}

#[test]
fn unchanged_builds_send_nothing() {
    let mut h = Harness::default();
    let (root, ..) = sample(&mut h.ui);
    h.build(root);

    let update = h.build(root);
    assert!(update.is_empty());
}

#[test]
fn marked_nodes_are_resolved_again() {
    let mut h = Harness::default();
    let (root, card, _) = sample(&mut h.ui);
    h.build(root);

    // Roles are not layout changes: they are marked in the tree.
    h.ui.set_props(card, (Role(AxRole::Region),));
    h.a11y.mark_node(&h.inputs, card);
    let update = h.build(root);

    assert_eq!(Harness::ids(&update), [node_id(h.key(card))]);
    assert_eq!(update[0].1.role(), AxRole::Region);
}

#[test]
fn paint_changes_update_transforms() {
    let mut h = Harness::default();
    let (root, card, _) = sample(&mut h.ui);
    h.build(root);

    // The text and its runs are placed in the card, so only the card moves.
    h.ui.set_props(card, (Transform(Affine::translate((20.0, 0.0))),));
    h.mark(card, DirtyFlags::NEED_LAYER);
    let update = h.build(root);

    assert_eq!(Harness::ids(&update), [node_id(h.key(card))]);
    let transform = update[0].1.transform().unwrap();
    assert_eq!(transform.as_coeffs()[4], 20.0);
}

#[test]
fn text_and_children_changes_update_the_tree() {
    let mut h = Harness::default();
    let (root, _, text) = sample(&mut h.ui);
    h.build(root);
    let runs = h.runs(text);

    // New text lays out new glyph runs, whose nodes keep their ids.
    h.ui.set_props(text, (Text("Bye".into()),));
    h.mark(text, DirtyFlags::NEED_FULL_LAYOUT);
    let update = h.build(root);
    assert!(!update.is_empty());
    assert_eq!(h.texts(), "Bye");
    assert_eq!(h.runs(text)[0], runs[0]);
    h.assert_consistent();

    // An added child is listed by its parent, and a removed one is deleted.
    let extra = h.ui.create(Node::Box, ());
    h.ui.append(root, extra);
    h.mark(root, DirtyFlags::NEED_CHILDREN);
    h.build(root);
    assert_eq!(h.node(h.root()).children().len(), 2);
    let extra_key = h.key(extra);
    h.assert_consistent();

    h.ui.delete(extra);
    h.mark(root, DirtyFlags::NEED_CHILDREN);
    let update = h.build(root);
    assert!(Harness::ids(&update).contains(&node_id(h.root())));
    assert_eq!(h.node(h.root()).children().len(), 1);
    assert!(h.a11y.get(extra_key).is_none());
    h.assert_consistent();
}

#[test]
fn changes_add_up_until_taken() {
    let mut h = Harness::default();
    let (root, card, _) = sample(&mut h.ui);
    h.build(root);

    // Two builds without taking an update.
    h.ui.set_props(card, (Role(AxRole::Region),));
    h.a11y.mark_node(&h.inputs, card);
    h.layout(root);
    h.ui.set_props(root, (Role(AxRole::Navigation),));
    h.a11y.mark_node(&h.inputs, root);
    h.layout(root);

    assert_eq!(h.a11y.changed().len(), 2);

    let update = h.a11y.take_update();
    let mut expected = vec![node_id(h.root()), node_id(h.key(card))];
    expected.sort();
    assert_eq!(Harness::ids(&update), expected);

    // Taking the update clears the changes.
    assert!(h.a11y.changed().is_empty());
    assert!(h.a11y.take_update().is_empty());
}

fn fixed(width: f64, height: f64) -> bx::Size {
    bx::Size(Size::new(
        AutoValue::Value(StyleUnit::Px(width)),
        AutoValue::Value(StyleUnit::Px(height)),
    ))
}

/// A 50x30 scroll container scrolled down by 10, holding three 40x20 rows,
/// the first with a text, then a text in the container's own flow. Returns
/// the root, the container, the rows and the texts.
fn scrolled(ui: &mut Ui) -> (NodeKey, NodeKey, [NodeKey; 3], [NodeKey; 2]) {
    let root = ui.create(Node::Box, ());
    let scroller = ui.create(
        Node::Box,
        (
            fixed(50.0, 30.0),
            bx::Overflow(Point::new(Overflow::Hidden, Overflow::Hidden)),
            ScrollTop(10.0),
        ),
    );
    ui.append(root, scroller);
    let rows = [(); 3].map(|_| {
        let row = ui.create(Node::Box, (fixed(40.0, 20.0),));
        ui.append(scroller, row);
        row
    });
    let inner = ui.create(Node::Text, (Text("row".into()),));
    ui.append(rows[0], inner);
    let flow = ui.create(Node::Text, (Text("after".into()),));
    ui.append(scroller, flow);

    (root, scroller, rows, [inner, flow])
}

#[test]
fn scroll_containers_expose_their_scroll_and_move_their_contents() {
    let mut h = Harness::default();
    let (root, scroller, rows, [_, flow]) = scrolled(&mut h.ui);
    let clipper = h.ui.create(
        Node::Box,
        (
            fixed(10.0, 10.0),
            bx::Overflow(Point::new(Overflow::Clip, Overflow::Visible)),
        ),
    );
    h.ui.append(root, clipper);
    h.build(root);

    let node = h.node(h.key(scroller));
    assert!(node.clips_children());
    assert_eq!(node.scroll_y(), Some(10.0));
    assert_eq!(node.scroll_y_min(), Some(0.0));
    let range = h.fragments.scroll_range(h.box_fragment(scroller)).unwrap();
    assert!(range.y > 30.0);
    assert_eq!(node.scroll_y_max(), Some(range.y));
    assert_eq!(node.scroll_x(), Some(0.0));
    assert_eq!(node.scroll_x_max(), Some(0.0));

    // Clipping without scrolling has no scroll.
    let node = h.node(h.key(clipper));
    assert!(node.clips_children());
    assert_eq!(node.scroll_y(), None);
    assert!(!h.node(h.key(root)).clips_children());

    // Rows move up by the scroll, inside the container's node.
    for (row, y) in rows.into_iter().zip([-10.0, 10.0, 30.0]) {
        assert_close(
            transform_of(h.node(h.key(row))),
            Affine::translate((0.0, y)),
        );
    }

    // A text in the container's flow has bounds in its space, scrolled.
    let bounds = h.node(h.key(flow)).bounds().unwrap();
    assert_eq!(bounds.y0, 60.0 - 10.0);
    h.assert_consistent();
}

#[test]
fn scrolling_updates_the_nodes_in_the_scrolled_space() {
    let mut h = Harness::default();
    let (root, scroller, rows, [inner, flow]) = scrolled(&mut h.ui);
    h.build(root);

    h.ui.set_props(scroller, (ScrollTop(20.0),));
    h.mark(scroller, DirtyFlags::NEED_LAYER);
    let update = h.build(root);

    // The container, its rows, and the text in its flow with its runs, which
    // are in its space. The text inside a row stays in the row's space.
    let mut expected: Vec<_> = [scroller, rows[0], rows[1], rows[2], flow]
        .into_iter()
        .map(|node| node_id(h.key(node)))
        .chain(h.runs(flow).into_iter().map(node_id))
        .collect();
    expected.sort();
    assert_eq!(Harness::ids(&update), expected);
    assert!(!Harness::ids(&update).contains(&node_id(h.key(inner))));

    assert_eq!(h.node(h.key(scroller)).scroll_y(), Some(20.0));
    assert_close(
        transform_of(h.node(h.key(rows[1]))),
        Affine::translate((0.0, 0.0)),
    );
    h.assert_consistent();
}
