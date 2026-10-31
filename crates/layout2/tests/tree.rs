use comet_layout2::{
    style::{DisplayInner, DisplayOuter},
    tree::{LayoutItemKey, LayoutItemTy, LayoutTree, builder::LayoutTreeBuilder},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

fn children(tree: &LayoutTree, key: LayoutItemKey) -> Vec<LayoutItemKey> {
    tree.items.children(key).collect()
}

fn check(tree: &LayoutTree, key: LayoutItemKey, ty: LayoutItemTy, node: Option<NodeKey>) {
    assert_eq!(tree.items[key].ty, ty);
    assert_eq!(tree.items[key].node, node);
}

#[test]
fn text_under_block_flow_gets_an_anonymous_inline_container() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = ui.create(Node::Text, ());
    let block = ui.create(Node::Box, ());
    let after = ui.create(Node::Text, (DisplayOuter::Block, DisplayInner::FlowRoot));

    for child in [before, block, after] {
        ui.append(root, child);
    }

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new().build_new(&ui, root, &mut tree).unwrap();

    check(&tree, key, LayoutItemTy::Block, Some(root));
    let top = children(&tree, key);
    assert_eq!(top.len(), 3);
    check(&tree, top[1], LayoutItemTy::Block, Some(block));

    for (wrapper, source) in [(top[0], before), (top[2], after)] {
        check(&tree, wrapper, LayoutItemTy::Inline, None);
        let text = children(&tree, wrapper);
        assert_eq!(text.len(), 1);
        check(&tree, text[0], LayoutItemTy::Text, Some(source));
        assert!(children(&tree, text[0]).is_empty());
    }

    assert_eq!(tree.items.len(), 6);
}

#[test]
fn text_root_is_a_single_text_leaf() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Text, (Text("Hello, 세계!".into()),));

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();

    check(&tree, key, LayoutItemTy::Text, Some(root));
    assert!(children(&tree, key).is_empty());
    assert_eq!(tree.items.len(), 1);
}

#[test]
fn block_children_keep_block_flow_even_when_their_own_content_is_inline() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let block = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, (Text("block text".into()),));

    ui.append(root, block);
    ui.append(block, text);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();

    check(&tree, key, LayoutItemTy::Block, Some(root));
    let child = children(&tree, key)[0];
    check(&tree, child, LayoutItemTy::Inline, Some(block));
    check(&tree, children(&tree, child)[0], LayoutItemTy::Text, Some(text));
    assert_eq!(tree.items.len(), 3);
}

#[test]
fn text_ignores_display_properties_and_ui_children() {
    for outer in [DisplayOuter::Block, DisplayOuter::Inline] {
        for inner in [DisplayInner::Flow, DisplayInner::FlowRoot, DisplayInner::Content] {
            let mut ui = Ui::new();
            let root = ui.create(Node::Box, ());
            let text = ui.create(Node::Text, (outer, inner));
            let ignored = ui.create(Node::Box, ());

            ui.append(root, text);
            ui.append(text, ignored);

            let mut tree = LayoutTree::new();
            let mut builder = LayoutTreeBuilder::new();
            let key = builder.build_new(&ui, root, &mut tree).unwrap();

            check(&tree, key, LayoutItemTy::Inline, Some(root));
            let leaf = children(&tree, key)[0];
            check(&tree, leaf, LayoutItemTy::Text, Some(text));
            assert!(children(&tree, leaf).is_empty());
            assert_eq!(tree.items.len(), 2);

            tree.clear();
            let key = builder.build_new(&ui, text, &mut tree).unwrap();

            check(&tree, key, LayoutItemTy::Text, Some(text));
            assert!(children(&tree, key).is_empty());
            assert_eq!(tree.items.len(), 1);
        }
    }
}

#[test]
fn inline_elements_keep_their_hierarchy_without_anonymous_wrappers() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, (DisplayOuter::Inline,));
    let inner = ui.create(Node::Text, (DisplayOuter::Inline, Text("hello".into())));
    let sibling = ui.create(Node::Box, (DisplayOuter::Inline,));

    ui.append(root, outer);
    ui.append(outer, inner);
    ui.append(root, sibling);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();

    let top = children(&tree, key);
    assert_eq!(top.len(), 2);
    check(&tree, key, LayoutItemTy::Inline, Some(root));
    check(&tree, top[0], LayoutItemTy::Inline, Some(outer));
    check(&tree, top[1], LayoutItemTy::Inline, Some(sibling));

    let nested = children(&tree, top[0]);
    assert_eq!(nested.len(), 1);
    check(&tree, nested[0], LayoutItemTy::Text, Some(inner));
    assert!(children(&tree, nested[0]).is_empty());
    assert_eq!(tree.items.len(), 4);
}

#[test]
fn blocks_split_nested_inline_objects_and_keep_empty_edge_fragments() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, (DisplayOuter::Inline,));
    let inner = ui.create(Node::Box, (DisplayOuter::Inline,));
    let a = ui.create(Node::Box, ());
    let b = ui.create(Node::Box, ());

    ui.append(root, outer);
    ui.append(outer, inner);
    ui.append(inner, a);
    ui.append(inner, b);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();

    let top = children(&tree, key);
    assert_eq!(top.len(), 4);
    check(&tree, top[1], LayoutItemTy::Block, Some(a));
    check(&tree, top[2], LayoutItemTy::Block, Some(b));

    for fragment in [top[0], top[3]] {
        check(&tree, fragment, LayoutItemTy::Inline, Some(outer));

        let nested = children(&tree, fragment);
        assert_eq!(nested.len(), 1);
        check(&tree, nested[0], LayoutItemTy::Inline, Some(inner));
        assert!(children(&tree, nested[0]).is_empty());
    }

    assert_eq!(tree.items.len(), 7);
}

#[test]
fn split_inline_root_has_one_anonymous_block_container() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline,));
    let a = ui.create(Node::Text, (DisplayOuter::Inline, Text("before".into())));
    let block = ui.create(Node::Box, ());
    let b = ui.create(Node::Text, (DisplayOuter::Inline, Text("after".into())));

    for child in [a, block, b] {
        ui.append(root, child);
    }

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();

    check(&tree, key, LayoutItemTy::Block, None);

    let top = children(&tree, key);
    assert_eq!(top.len(), 3);
    check(&tree, top[1], LayoutItemTy::Block, Some(block));

    for (fragment, source) in [(top[0], a), (top[2], b)] {
        check(&tree, fragment, LayoutItemTy::Inline, Some(root));

        let text_element = children(&tree, fragment)[0];
        check(&tree, text_element, LayoutItemTy::Text, Some(source));
        assert!(children(&tree, text_element).is_empty());
    }
}

#[test]
fn inline_block_has_an_anonymous_inline_wrapper_and_its_own_flow() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let atomic = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let block = ui.create(Node::Box, ());

    ui.append(root, atomic);
    ui.append(atomic, span);
    ui.append(span, block);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();

    let top = children(&tree, key);
    assert_eq!(top.len(), 1);
    check(&tree, top[0], LayoutItemTy::Inline, None);

    let body = children(&tree, top[0]);
    assert_eq!(body.len(), 1);
    check(&tree, body[0], LayoutItemTy::Block, Some(atomic));

    let fragments = children(&tree, body[0]);
    assert_eq!(fragments.len(), 3);
    check(&tree, fragments[0], LayoutItemTy::Inline, Some(span));
    check(&tree, fragments[1], LayoutItemTy::Block, Some(block));
    check(&tree, fragments[2], LayoutItemTy::Inline, Some(span));
}

#[test]
fn content_does_not_build_ui_children_and_empty_boxes_need_no_wrappers() {
    for outer in [DisplayOuter::Block, DisplayOuter::Inline] {
        let mut ui = Ui::new();
        let root = ui.create(Node::Box, (outer, DisplayInner::Content));
        let ignored = ui.create(Node::Text, (Text("ignored".into()),));

        ui.append(root, ignored);

        let mut tree = LayoutTree::new();
        let key = LayoutTreeBuilder::new()
            .build_new(&ui, root, &mut tree)
            .unwrap();

        if outer == DisplayOuter::Block {
            assert_eq!(tree.items.len(), 1);
            assert!(children(&tree, key).is_empty());
        } else {
            assert_eq!(tree.items.len(), 2);
            check(&tree, key, LayoutItemTy::Inline, None);

            let body = children(&tree, key)[0];
            check(&tree, body, LayoutItemTy::Block, Some(root));
            assert!(children(&tree, body).is_empty());
        }
    }
}

#[test]
fn inline_root_and_deep_inline_hierarchy_need_no_anonymous_items() {
    let mut ui = Ui::new();
    let mut root = ui.create(Node::Text, (DisplayOuter::Inline, Text("leaf".into())));

    for _ in 0..2000 {
        let parent = ui.create(Node::Box, (DisplayOuter::Inline,));

        ui.append(parent, root);
        root = parent;
    }

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();

    check(&tree, key, LayoutItemTy::Inline, Some(root));
    assert_eq!(tree.items.len(), 2001);

    let mut next = Some(key);

    while let Some(key) = next {
        assert!(tree.items[key].node.is_some());
        next = tree.items.first_child(key);
    }
}
