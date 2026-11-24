use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

use crate::{
    dirty_map::DirtyMap,
    fragment2::inline::input::{InputItem, InlinePrelayout},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{bx, geometry::Size},
};

fn build(ui: &Ui, root: NodeKey) -> (LayoutInputTree, LayoutInputKey) {
    let mut tree = LayoutInputTree::new();
    let key = LayoutInputTreeBuilder::new()
        .build(
            ui,
            &mut tree,
            &mut DirtyMap::new(),
            BuildRequest::new(root, 16.0, Size::new(400.0, 600.0)),
        )
        .unwrap();

    (tree, key)
}

fn text(ui: &mut Ui, value: &str) -> NodeKey {
    ui.create(Node::Text, (Text(value.into()),))
}

#[test]
fn flattens_nested_inline_content() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before ");
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let inner = text(&mut ui, "inner");
    let empty = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let badge = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    let badge_text = text(&mut ui, "badge");
    let block = ui.create(Node::Box, ());
    let after = text(&mut ui, " after");

    ui.append(root, before);
    ui.append(root, span);
    ui.append(span, inner);
    ui.append(span, empty);
    ui.append(span, badge);
    ui.append(badge, badge_text);
    ui.append(span, block);
    ui.append(root, after);

    let (tree, key) = build(&ui, root);
    let item = |node| tree.item_for_node(node).unwrap();

    use InputItem::*;
    assert_eq!(
        InlinePrelayout::new(&tree, key).collect::<Vec<_>>(),
        [
            Text(item(before)),
            Push(item(span)),
            Text(item(inner)),
            Push(item(empty)),
            Pop,
            Atomic(item(badge)),
            Atomic(item(block)),
            Pop,
            Text(item(after)),
        ]
    );
}

#[test]
fn closes_every_inline_element_at_the_end() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let inner = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let leaf = text(&mut ui, "leaf");

    ui.append(root, outer);
    ui.append(outer, inner);
    ui.append(inner, leaf);

    let (tree, key) = build(&ui, root);
    let item = |node| tree.item_for_node(node).unwrap();

    use InputItem::*;
    assert_eq!(
        InlinePrelayout::new(&tree, key).collect::<Vec<_>>(),
        [
            Push(item(outer)),
            Push(item(inner)),
            Text(item(leaf)),
            Pop,
            Pop
        ]
    );
}

#[test]
fn text_root_yields_itself() {
    let mut ui = Ui::new();
    let root = text(&mut ui, "alone");

    let (tree, key) = build(&ui, root);

    assert_eq!(
        InlinePrelayout::new(&tree, key).collect::<Vec<_>>(),
        [InputItem::Text(key)]
    );
}

#[test]
fn empty_root_yields_nothing() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());

    let (tree, key) = build(&ui, root);

    assert_eq!(InlinePrelayout::new(&tree, key).next(), None);
}
