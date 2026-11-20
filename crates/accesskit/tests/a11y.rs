use accesskit::{NodeId, Role as AxRole};
use comet_accesskit::{
    A11yTree, A11yTreeBuilder,
    style::Role,
    tree::{fragment_key, node_id},
};
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{FragmentItem, FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint,
        bx::Padding,
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

    /// Labels of the glyph run nodes in the tree, in tree order.
    fn labels(&self) -> String {
        let root = self.root();
        self.fragments
            .items
            .descendants(root)
            .filter(|&key| {
                matches!(
                    self.fragments.items[key].item,
                    FragmentItem::GlyphRun { .. }
                )
            })
            .filter_map(|key| self.a11y.get(key)?.node.label().map(str::to_owned))
            .collect()
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
fn mirrors_the_fragment_tree() {
    let mut h = Harness::default();
    let (root, card, _) = sample(&mut h.ui);
    let update = h.build(root);

    // Every fragment has a node, sent in the first update with tree info.
    let root_fragment = h.root();
    let count = h.fragments.items.descendants(root_fragment).count();
    assert_eq!(update.len(), count);
    assert!(Harness::ids(&update).contains(&node_id(root_fragment)));

    for key in h.fragments.items.descendants(root_fragment) {
        let node = &h.a11y.get(key).unwrap().node;
        let children: Vec<_> = h.fragments.items.children(key).map(node_id).collect();
        assert_eq!(node.children(), children);

        let size = h.fragments.items[key].layout.size;
        assert_eq!(
            node.bounds(),
            Some(accesskit::Rect::new(0.0, 0.0, size.width, size.height))
        );
    }

    let root_node = h.node(node_id(root_fragment));
    assert_eq!(root_node.role(), AxRole::Main);

    // The card is translated by its transform and placed inside the root.
    let card_item = h.inputs.item_for_node(card).unwrap();
    let card_fragment = h.fragments.box_fragment(card_item).unwrap();
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

    // The card's line box holds a labelled glyph run.
    assert_eq!(h.labels(), "Hello comet");
    let root = h.root();
    for key in h.fragments.items.descendants(root) {
        if matches!(h.fragments.items[key].item, FragmentItem::GlyphRun { .. }) {
            assert_eq!(h.a11y.get(key).unwrap().node.role(), AxRole::TextRun);
        }
    }
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
    h.a11y.mark_node(&h.inputs, &h.fragments, card);
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
    assert_eq!(h.labels(), "Bye");
    assert_consistent(&h);

    // A removed child leaves its parent's children.
    let extra = h.ui.create(Node::Box, ());
    h.ui.append(root, extra);
    h.mark(root, DirtyFlags::NEED_CHILDREN);
    h.build(root);
    assert_consistent(&h);

    h.ui.delete(extra);
    h.mark(root, DirtyFlags::NEED_CHILDREN);
    let update = h.build(root);
    let root_fragment = h.root();
    assert!(Harness::ids(&update).contains(&node_id(root_fragment)));
    assert_eq!(h.node(node_id(root_fragment)).children().len(), 1);
    assert_consistent(&h);
}

/// Every node lists the nodes of its child fragments, as after a full build.
fn assert_consistent(h: &Harness) {
    let root = h.root();
    for key in h.fragments.items.descendants(root) {
        let node = &h.a11y.get(key).expect("every fragment has a node").node;
        let children: Vec<_> = h.fragments.items.children(key).map(node_id).collect();
        assert_eq!(node.children(), children, "children of {key:?}");
    }
}

#[test]
fn changes_add_up_until_taken() {
    let mut h = Harness::default();
    let (root, card, _) = sample(&mut h.ui);
    h.build(root);

    // Two builds without taking an update.
    h.ui.set_props(card, (Role(AxRole::Group),));
    h.a11y.mark_node(&h.inputs, &h.fragments, card);
    h.layout(root);
    h.ui.set_props(root, (Role(AxRole::Navigation),));
    h.a11y.mark_node(&h.inputs, &h.fragments, root);
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
