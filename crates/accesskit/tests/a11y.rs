use accesskit::{NodeId, Role as AxRole};
use comet_accesskit::{
    A11yTree, A11yTreeBuilder,
    style::Role,
    tree::{fragment_key, node_id},
};
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint,
        bx::{DisplayOuter, Padding},
        geometry::{Rect, Size, StyleUnit},
    },
};
use comet_paint::{
    prepaint::{builder::PrepaintTreeBuilder, tree::PrepaintTree},
    style::Transform,
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use kurbo::Affine;

const VIEWPORT: Size<f64> = Size::new(400.0, 300.0);

/// The layout steps and the accessibility tree of one UI.
#[derive(Default)]
struct Harness {
    ui: Ui,
    inputs: LayoutInputTree,
    dirty: DirtyMap,
    fragments: FragmentTree,
    prepaint: PrepaintTree,
    a11y: A11yTree,
    input_builder: LayoutInputTreeBuilder,
    fragment_builder: FragmentTreeBuilder,
    prepaint_builder: PrepaintTreeBuilder,
    a11y_builder: A11yTreeBuilder,
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
            a11y,
            ..
        } = self;
        let input_root = self
            .input_builder
            .build(ui, inputs, dirty, BuildRequest::new(root, 16.0, VIEWPORT))
            .expect("input tree");
        let constraint = LayoutConstraint::new_root(VIEWPORT, 16.0);
        let fragment_root = self
            .fragment_builder
            .build(ui, inputs, dirty, input_root, fragments, &constraint)
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
        self.a11y_builder
            .build(ui, inputs, dirty, fragments, prepaint, a11y, input_root)
            .expect("accessibility tree");
        dirty.clear(inputs, input_root);
        self.input_root = Some(input_root);
    }

    /// The root fragment of the last build.
    fn root(&self) -> FragmentKey {
        self.fragments
            .box_fragment(self.input_root.unwrap())
            .unwrap()
    }

    fn mark(&mut self, node: NodeKey, flags: DirtyFlags) {
        self.dirty.mark_node(&self.inputs, node, flags);
    }

    fn node(&self, id: NodeId) -> &accesskit::Node {
        &self.a11y.get(fragment_key(id)).expect("node").node
    }

    /// Ids of the updated nodes.
    fn ids(update: &[(NodeId, accesskit::Node)]) -> Vec<NodeId> {
        let mut ids: Vec<_> = update.iter().map(|(id, _)| *id).collect();
        ids.sort();
        ids
    }

    /// Values of the nodes in the accessibility tree, in tree order.
    fn texts(&self) -> String {
        let mut labels = String::new();
        let mut pending = vec![node_id(self.root())];
        while let Some(id) = pending.pop() {
            let node = self.node(id);
            labels.extend(node.value());
            pending.extend(node.children().iter().rev());
        }
        labels
    }

    /// Check that every node is as a full build would resolve it.
    fn assert_consistent(&self) {
        let mut fresh = A11yTree::new();
        A11yTreeBuilder::new()
            .build(
                &self.ui,
                &self.inputs,
                &DirtyMap::new(),
                &self.fragments,
                &self.prepaint,
                &mut fresh,
                self.input_root.unwrap(),
            )
            .expect("full build");

        let mut pending = vec![node_id(self.root())];
        while let Some(id) = pending.pop() {
            let expected = &fresh.get(fragment_key(id)).unwrap().node;
            assert_eq!(self.node(id), expected, "node {id:?}");
            pending.extend(expected.children());
        }
    }
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

#[test]
fn follows_the_ui_tree() {
    let mut h = Harness::default();
    let (root, card, text) = sample(&mut h.ui);
    let update = h.build(root);

    let fragment = |h: &Harness, node| {
        let item = h.inputs.item_for_node(node).unwrap();
        h.fragments.fragments(item).collect::<Vec<_>>()
    };
    let root_fragment = h.root();
    let card_fragment = fragment(&h, card)[0];
    let runs = fragment(&h, text);
    assert!(!runs.is_empty());

    // Only the fragments of items have nodes, all sent in the first update.
    let mut expected: Vec<_> = [root_fragment, card_fragment]
        .into_iter()
        .chain(runs.iter().copied())
        .map(node_id)
        .collect();
    expected.sort();
    assert_eq!(Harness::ids(&update), expected);

    let root_node = h.node(node_id(root_fragment));
    assert_eq!(root_node.role(), AxRole::Main);
    assert_eq!(root_node.children(), [node_id(card_fragment)]);

    // The card is translated by its transform and placed inside the root.
    let card_node = h.node(node_id(card_fragment));
    assert_eq!(card_node.role(), AxRole::GenericContainer);
    assert_eq!(
        card_node.transform(),
        Some(&accesskit::Affine::new(
            h.prepaint
                .fragment(card_fragment)
                .unwrap()
                .transform
                .as_coeffs()
        ))
    );
    let size = h.fragments.items[card_fragment].layout.size;
    assert_eq!(
        card_node.bounds(),
        Some(accesskit::Rect::new(0.0, 0.0, size.width, size.height))
    );

    // Its glyph runs are its children, without the line box between them,
    // and placed through it.
    assert_eq!(
        card_node.children(),
        runs.iter().copied().map(node_id).collect::<Vec<_>>()
    );
    let run = runs[0];
    let line = h.fragments.items.parent(run).unwrap();
    let placed =
        h.prepaint.fragment(line).unwrap().transform * h.prepaint.fragment(run).unwrap().transform;
    let run_node = h.node(node_id(run));
    assert_eq!(run_node.role(), AxRole::TextRun);
    assert_eq!(
        run_node.transform(),
        Some(&accesskit::Affine::new(placed.as_coeffs()))
    );
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

    // The block is a child of the span's first fragment, placed like its
    // box.
    let item = h.inputs.item_for_node(block).unwrap();
    let block_fragment = h.fragments.box_fragment(item).unwrap();
    let span_item = h.inputs.item_for_node(span).unwrap();
    let span_fragment = h.fragments.fragments(span_item).next().unwrap();
    assert!(
        h.node(node_id(span_fragment))
            .children()
            .contains(&node_id(block_fragment))
    );
    let transform = |key| {
        core::iter::successors(Some(key), |&key| h.fragments.items.parent(key))
            .fold(Affine::IDENTITY, |transform, key| {
                h.prepaint.fragment(key).unwrap().transform * transform
            })
    };
    let expected = transform(span_fragment).inverse() * transform(block_fragment);
    let actual = h.node(node_id(block_fragment)).transform().unwrap();
    for (actual, expected) in actual.as_coeffs().iter().zip(expected.as_coeffs()) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }
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
    let (root, card, text) = sample(&mut h.ui);
    h.ui.set_props(card, (Role(AxRole::Label),));
    let update = h.build(root);

    let item = h.inputs.item_for_node(text).unwrap();
    let runs: Vec<_> = h.fragments.fragments(item).collect();
    for (index, &run) in runs.iter().enumerate() {
        let node = h.node(node_id(run));
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
        assert_eq!(
            node.next_on_line(),
            runs.get(index + 1).copied().map(node_id)
        );
    }

    // A label exposes the text of its runs, as platform adapters read it.
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
    let card_fragment = h
        .fragments
        .box_fragment(h.inputs.item_for_node(card).unwrap())
        .unwrap();
    let label = tree
        .state()
        .root()
        .children()
        .find(|node| node.locate().0 == node_id(card_fragment))
        .unwrap();
    assert_eq!(label.value().as_deref(), Some("Hello comet"));
    // Adapters name a label with its value.
    assert!(label.label_comes_from_value());
}

#[test]
fn roles_default_by_node_kind() {
    let mut h = Harness::default();
    let (root, card, text) = sample(&mut h.ui);
    // An explicit role on a text does not reach its glyph runs.
    h.ui.set_props(text, (Role(AxRole::Heading),));
    h.build(root);

    let card_item = h.inputs.item_for_node(card).unwrap();
    let card_fragment = h.fragments.box_fragment(card_item).unwrap();
    assert_eq!(
        h.node(node_id(card_fragment)).role(),
        AxRole::GenericContainer
    );
    let item = h.inputs.item_for_node(text).unwrap();
    for run in h.fragments.fragments(item) {
        assert_eq!(h.node(node_id(run)).role(), AxRole::TextRun);
    }
}

#[test]
fn texts_with_boxes_are_labels_of_their_runs() {
    let mut h = Harness::default();
    let root = h.ui.create(Node::Text, (Text("Boxed text".into()),));
    h.build(root);

    // The text's box holds its glyph runs, which it is not linked with.
    let root_fragment = h.root();
    let root_node = h.node(node_id(root_fragment));
    assert_eq!(root_node.role(), AxRole::Label);
    assert!(!root_node.children().is_empty());
    for &child in root_node.children() {
        assert_eq!(h.node(child).role(), AxRole::TextRun);
    }
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
    h.ui.set_props(card, (Role(AxRole::Group),));
    h.a11y.mark_node(&h.inputs, card);
    let update = h.build(root);

    let card_fragment = h
        .fragments
        .box_fragment(h.inputs.item_for_node(card).unwrap())
        .unwrap();
    assert_eq!(Harness::ids(&update), [node_id(card_fragment)]);
    assert_eq!(update[0].1.role(), AxRole::Group);
}

#[test]
fn paint_changes_update_transforms() {
    let mut h = Harness::default();
    let (root, card, _) = sample(&mut h.ui);
    h.build(root);

    h.ui.set_props(card, (Transform(Affine::translate((20.0, 0.0))),));
    h.mark(card, DirtyFlags::NEED_PRE_PAINT);
    let update = h.build(root);

    let card_fragment = h
        .fragments
        .box_fragment(h.inputs.item_for_node(card).unwrap())
        .unwrap();
    assert_eq!(Harness::ids(&update), [node_id(card_fragment)]);
    let transform = update[0].1.transform().unwrap();
    assert_eq!(transform.as_coeffs()[4], 20.0);
}

#[test]
fn text_and_children_changes_update_the_tree() {
    let mut h = Harness::default();
    let (root, _, text) = sample(&mut h.ui);
    h.build(root);

    // New text lays out new lines and glyph runs, listed by their parents.
    h.ui.set_props(text, (Text("Bye".into()),));
    h.mark(text, DirtyFlags::NEED_FULL_LAYOUT);
    let update = h.build(root);
    assert!(!update.is_empty());
    assert_eq!(h.texts(), "Bye");
    h.assert_consistent();

    // A removed child leaves its parent's children.
    let extra = h.ui.create(Node::Box, ());
    h.ui.append(root, extra);
    h.mark(root, DirtyFlags::NEED_CHILDREN);
    h.build(root);
    h.assert_consistent();

    h.ui.delete(extra);
    h.mark(root, DirtyFlags::NEED_CHILDREN);
    let update = h.build(root);
    let root_fragment = h.root();
    assert!(Harness::ids(&update).contains(&node_id(root_fragment)));
    assert_eq!(h.node(node_id(root_fragment)).children().len(), 1);
    h.assert_consistent();
}

#[test]
fn changes_add_up_until_taken() {
    let mut h = Harness::default();
    let (root, card, _) = sample(&mut h.ui);
    h.build(root);

    // Two builds without taking an update.
    h.ui.set_props(card, (Role(AxRole::Group),));
    h.a11y.mark_node(&h.inputs, card);
    h.layout(root);
    h.ui.set_props(root, (Role(AxRole::Navigation),));
    h.a11y.mark_node(&h.inputs, root);
    h.layout(root);

    let card_fragment = h
        .fragments
        .box_fragment(h.inputs.item_for_node(card).unwrap())
        .unwrap();
    assert_eq!(h.a11y.changed().len(), 2);

    let update = h.a11y.take_update();
    let mut expected = vec![node_id(h.root()), node_id(card_fragment)];
    expected.sort();
    assert_eq!(Harness::ids(&update), expected);

    // Taking the update clears the changes.
    assert!(h.a11y.changed().is_empty());
    assert!(h.a11y.take_update().is_empty());
}
