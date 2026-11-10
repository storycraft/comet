use std::ops::Deref;

use comet_layout::{
    fragment::{FragmentBox, FragmentItem, FragmentKey, FragmentTree, FragmentTreeBuilder},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Rect, Size, StyleUnit},
    },
};
use comet_layout_flex::{Flex, styles as flex};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

const VIEWPORT: Size<f64> = Size::new(800.0, 600.0);

fn size(width: f64, height: f64) -> bx::Size {
    bx::Size(Size::new(
        AutoValue::Value(StyleUnit::Px(width)),
        AutoValue::Value(StyleUnit::Px(height)),
    ))
}

/// Input and fragment trees of one root, built together.
#[derive(Default)]
struct Layout {
    inputs: LayoutInputTree,
    fragments: FragmentTree,
    input_builder: LayoutInputTreeBuilder,
    builder: FragmentTreeBuilder,
}

impl Deref for Layout {
    type Target = FragmentTree;

    fn deref(&self) -> &FragmentTree {
        &self.fragments
    }
}

impl Layout {
    /// Build or update the trees of `root`, then clear the input flags.
    fn build(&mut self, ui: &Ui, root: NodeKey) -> FragmentKey {
        let key = self
            .input_builder
            .build(
                ui,
                &mut self.inputs,
                BuildRequest::new(root, 16.0, VIEWPORT),
            )
            .unwrap();
        let fragment = self
            .builder
            .build(
                ui,
                &self.inputs,
                key,
                &mut self.fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0),
            )
            .unwrap();
        self.inputs.clear_flags(key);

        fragment
    }

    fn item(&self, node: NodeKey) -> LayoutInputKey {
        self.inputs.item_for_node(node).unwrap()
    }

    fn node(&self, key: FragmentKey) -> Option<NodeKey> {
        self.inputs
            .items
            .get(self.fragments.items[key].source)
            .map(|item| item.node)
    }
}

fn layout(ui: &Ui, root: NodeKey) -> (Layout, FragmentKey) {
    let mut layout = Layout::default();
    let key = layout.build(ui, root);

    (layout, key)
}

fn box_for(tree: &Layout, root: FragmentKey, node: NodeKey) -> FragmentBox {
    let key = tree.box_fragment(tree.item(node)).unwrap();

    assert!(tree.items.descendants(root).any(|other| other == key));
    assert_eq!(tree.items[key].item, FragmentItem::Box);

    tree.items[key].layout
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
}

#[test]
fn grows_and_shrinks_items_with_gaps() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(Box::new(Flex)),
            size(300.0, 100.0),
            flex::Gap(Size::new(StyleUnit::Px(10.0), StyleUnit::ZERO)),
        ),
    );
    let a = ui.create(Node::Box, (size(50.0, 20.0), flex::FlexGrow(1.0)));
    let b = ui.create(Node::Box, (size(50.0, 20.0), flex::FlexGrow(2.0)));

    ui.append(root, a);
    ui.append(root, b);

    let (tree, key) = layout(&ui, root);
    let first = box_for(&tree, key, a);
    let second = box_for(&tree, key, b);

    close(first.size.width, 50.0 + 190.0 / 3.0);
    close(second.size.width, 50.0 + 380.0 / 3.0);
    close(second.location.x, first.size.width + 10.0);

    ui.set_props(root, (size(70.0, 100.0),));

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, a).size.width, 30.0);
    close(box_for(&tree, key, b).location.x, 40.0);
    close(box_for(&tree, key, b).size.width, 30.0);
}

#[test]
fn percentages_use_content_size_and_padding_is_applied_once() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(Box::new(Flex)),
            size(224.0, 100.0),
            bx::Padding(Rect::new_all(StyleUnit::Px(10.0))),
            bx::Border(Rect::new_all(StyleUnit::Px(2.0))),
            flex::Gap(Size::new(StyleUnit::Percent(0.1), StyleUnit::ZERO)),
        ),
    );
    let a = ui.create(
        Node::Box,
        (
            size(1.0, 20.0),
            flex::FlexBasis(AutoValue::Value(StyleUnit::Percent(0.25))),
        ),
    );
    let b = ui.create(
        Node::Box,
        (
            size(1.0, 20.0),
            flex::FlexBasis(AutoValue::Value(StyleUnit::Percent(0.25))),
        ),
    );

    ui.append(root, a);
    ui.append(root, b);

    for (box_sizing, width, height) in [
        (taffy::BoxSizing::BorderBox, 224.0, 100.0),
        (taffy::BoxSizing::ContentBox, 200.0, 76.0),
    ] {
        ui.set_props(root, (bx::BoxSizing(box_sizing), size(width, height)));

        let (tree, key) = layout(&ui, root);
        let a = box_for(&tree, key, a);
        let b = box_for(&tree, key, b);

        close(tree.items[key].layout.size.width, 224.0);
        close(tree.items[key].layout.size.height, 100.0);
        close(a.location.x, 12.0);
        close(a.location.y, 12.0);
        close(a.size.width, 50.0);
        close(b.location.x, 82.0);
        close(b.size.width, 50.0);
    }
}

#[test]
fn wraps_and_aligns_rows_and_supports_column_reverse() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(Box::new(Flex)),
            size(130.0, 100.0),
            flex::FlexWrap(taffy::FlexWrap::Wrap),
            flex::Gap(Size::new(StyleUnit::Px(10.0), StyleUnit::Px(5.0))),
            flex::AlignContent(taffy::AlignContent::START),
        ),
    );
    let children: Vec<_> = (0..3)
        .map(|_| {
            let node = ui.create(Node::Box, (size(60.0, 20.0),));
            ui.append(root, node);

            node
        })
        .collect();

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, children[1]).location.x, 70.0);
    close(box_for(&tree, key, children[2]).location.y, 25.0);

    ui.set_props(
        root,
        (
            flex::FlexDirection(taffy::FlexDirection::ColumnReverse),
            flex::FlexWrap(taffy::FlexWrap::NoWrap),
            flex::AlignItems(taffy::AlignItems::CENTER),
        ),
    );

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, children[0]).location.y, 80.0);
    close(box_for(&tree, key, children[1]).location.y, 55.0);
    close(box_for(&tree, key, children[2]).location.x, 35.0);
}

#[test]
fn alignment_and_item_override_use_fixed_cross_size() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(Box::new(Flex)),
            size(200.0, 100.0),
            flex::JustifyContent(taffy::JustifyContent::SPACE_BETWEEN),
            flex::AlignItems(taffy::AlignItems::CENTER),
        ),
    );
    let a = ui.create(Node::Box, (size(40.0, 20.0),));
    let b = ui.create(
        Node::Box,
        (
            size(40.0, 20.0),
            flex::AlignSelf(Some(taffy::AlignSelf::END)),
        ),
    );

    ui.append(root, a);
    ui.append(root, b);

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, a).location.y, 40.0);
    close(box_for(&tree, key, b).location.x, 160.0);
    close(box_for(&tree, key, b).location.y, 80.0);
}

#[test]
fn font_and_viewport_units_use_the_source_node_context() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(Box::new(Flex)),
            size(500.0, 100.0),
            bx::FontSize(StyleUnit::Px(20.0)),
            flex::Gap(Size::new(StyleUnit::Em(1.0), StyleUnit::ZERO)),
        ),
    );
    let a = ui.create(
        Node::Box,
        (
            bx::FontSize(StyleUnit::Px(30.0)),
            flex::FlexBasis(AutoValue::Value(StyleUnit::Em(2.0))),
        ),
    );
    let b = ui.create(
        Node::Box,
        (flex::FlexBasis(AutoValue::Value(StyleUnit::Rem(2.0))),),
    );
    let c = ui.create(
        Node::Box,
        (flex::FlexBasis(AutoValue::Value(StyleUnit::Vw(0.1))),),
    );
    let d = ui.create(
        Node::Box,
        (flex::FlexBasis(AutoValue::Value(StyleUnit::Vh(0.1))),),
    );

    for child in [a, b, c, d] {
        ui.append(root, child);
    }

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, a).size.width, 60.0);
    close(box_for(&tree, key, b).size.width, 32.0);
    close(box_for(&tree, key, b).location.x, 80.0);
    close(box_for(&tree, key, c).size.width, 80.0);
    close(box_for(&tree, key, d).size.width, 60.0);
}

#[test]
fn wrapped_text_and_inline_children_use_source_flex_properties() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(Box::new(Flex)),
            size(300.0, 100.0),
        ),
    );
    let text = ui.create(
        Node::Text,
        (
            Text("first item with wrapping words".into()),
            flex::FlexBasis(AutoValue::Value(StyleUnit::Percent(0.3))),
        ),
    );
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline, flex::FlexGrow(1.0)));
    let inside = ui.create(Node::Text, (Text("second item".into()),));

    ui.append(root, text);
    ui.append(root, span);
    ui.append(span, inside);

    let (tree, key) = layout(&ui, root);
    let children: Vec<_> = tree.items.children(key).collect();

    assert_eq!(children.len(), 2);
    close(tree.items[children[0]].layout.size.width, 90.0);
    close(tree.items[children[1]].layout.location.x, 90.0);
    close(tree.items[children[1]].layout.size.width, 210.0);

    for source in [text, inside] {
        assert!(tree.items.descendants(key).any(|key| {
            tree.node(key) == Some(source)
                && matches!(tree.items[key].item, FragmentItem::GlyphRun { .. })
        }));
    }
}

#[test]
fn nested_flex_and_incremental_style_changes_match_fresh_layout() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(Box::new(Flex)),
            size(300.0, 100.0),
        ),
    );
    let nested = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(Box::new(Flex)),
            flex::FlexGrow(1.0),
        ),
    );
    let a = ui.create(Node::Box, (flex::FlexGrow(1.0),));
    let b = ui.create(Node::Box, (flex::FlexGrow(1.0),));
    let sibling = ui.create(Node::Box, (size(50.0, 20.0),));

    ui.append(root, nested);
    ui.append(root, sibling);
    ui.append(nested, a);
    ui.append(nested, b);

    let mut retained = Layout::default();

    for step in 0..3 {
        if step == 1 {
            ui.set_props(
                nested,
                (flex::Gap(Size::new(StyleUnit::Px(10.0), StyleUnit::ZERO)),),
            );
            ui.set_props(b, (flex::FlexGrow(2.0),));
            let (nested, b) = (retained.item(nested), retained.item(b));
            retained.inputs.mark_dirty(nested);
            retained.inputs.mark_dirty(b);
        } else if step == 2 {
            ui.prepend(nested, b);
            let nested = retained.item(nested);
            retained.inputs.mark_dirty(nested);
        }

        let current = retained.build(&ui, root);
        let (fresh, fresh_root) = layout(&ui, root);

        for node in [root, nested, a, b, sibling] {
            assert_eq!(
                box_for(&retained, current, node),
                box_for(&fresh, fresh_root, node)
            );
        }

        close(box_for(&retained, current, nested).size.width, 250.0);

        if step == 0 {
            close(box_for(&retained, current, a).size.width, 125.0);
        } else {
            close(box_for(&retained, current, a).size.width, 80.0);
            close(box_for(&retained, current, b).size.width, 160.0);
        }
    }
}

#[test]
fn absolute_children_and_descendants_stay_out_of_flex_sizing() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(Box::new(Flex)),
            size(200.0, 100.0),
        ),
    );
    let block = ui.create(Node::Box, (size(50.0, 40.0),));
    let absolute = ui.create(
        Node::Box,
        (
            size(20.0, 10.0),
            bx::Position(taffy::Position::Absolute),
            bx::Inset(Rect::new_invidiual(
                AutoValue::Value(StyleUnit::Px(7.0)),
                AutoValue::Auto,
                AutoValue::Auto,
                AutoValue::Value(StyleUnit::Px(11.0)),
            )),
        ),
    );
    let nested = ui.create(
        Node::Box,
        (
            size(5.0, 5.0),
            bx::Position(taffy::Position::Absolute),
            bx::Inset(Rect::new_all(AutoValue::Value(StyleUnit::Px(3.0)))),
        ),
    );

    ui.append(root, block);
    ui.append(root, absolute);
    ui.append(block, nested);

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, block).size.width, 50.0);
    close(box_for(&tree, key, absolute).location.x, 11.0);
    close(box_for(&tree, key, absolute).location.y, 7.0);
    close(box_for(&tree, key, nested).location.x, 3.0);
    close(box_for(&tree, key, nested).location.y, 3.0);
}

#[test]
fn text_items_align_by_their_first_baseline() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(Box::new(Flex)),
            size(300.0, 100.0),
            flex::AlignItems(taffy::AlignItems::BASELINE),
        ),
    );
    let a = ui.create(
        Node::Text,
        (Text("small".into()), bx::FontSize(StyleUnit::Px(12.0))),
    );
    let b = ui.create(
        Node::Text,
        (Text("large".into()), bx::FontSize(StyleUnit::Px(32.0))),
    );

    ui.append(root, a);
    ui.append(root, b);

    let (tree, key) = layout(&ui, root);
    let baseline = |node| {
        let mut current = tree
            .items
            .descendants(key)
            .find(|&key| {
                tree.node(key) == Some(node)
                    && matches!(tree.items[key].item, FragmentItem::GlyphRun { .. })
            })
            .unwrap();
        let mut y = tree.items[current].layout.baseline.unwrap();

        while current != key {
            y += tree.items[current].layout.location.y;
            current = tree.items.parent(current).unwrap();
        }

        y
    };

    close(baseline(a), baseline(b));
}
