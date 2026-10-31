use comet_layout2::{
    style::{DisplayInner, DisplayOuter},
    tree::{LayoutItem, LayoutItemKey, LayoutItemTy, LayoutTree, builder::LayoutTreeBuilder},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

fn block(node: Option<NodeKey>) -> LayoutItem {
    LayoutItem {
        ty: LayoutItemTy::Block,
        node,
    }
}

fn keys(tree: &LayoutTree, root: LayoutItemKey) -> Vec<LayoutItemKey> {
    let mut pending = vec![root];
    let mut result = Vec::new();

    while let Some(key) = pending.pop() {
        result.push(key);
        pending.extend(tree.items.children(key).rev());
    }

    result
}

fn snapshot(tree: &LayoutTree, root: LayoutItemKey) -> Vec<(usize, LayoutItem)> {
    let mut pending = vec![(root, 0)];
    let mut result = Vec::new();

    while let Some((key, depth)) = pending.pop() {
        result.push((depth, tree.items[key]));
        pending.extend(tree.items.children(key).rev().map(|key| (key, depth + 1)));
    }

    result
}

fn assert_fresh(ui: &Ui, source: NodeKey, tree: &LayoutTree, root: LayoutItemKey) {
    let mut fresh = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(ui, source, &mut fresh)
        .unwrap();

    assert_eq!(snapshot(tree, root), snapshot(&fresh, key));
}

#[test]
fn public_item_api_deletes_owned_subtrees_and_ignores_stale_keys() {

    let mut tree = LayoutTree::new();
    let root = tree.add_item(block(None));
    let child = tree.append_item(root, block(None)).unwrap();
    let leaf = tree
        .append_item(
            child,
            LayoutItem {
                node: None,
                ty: LayoutItemTy::Text,
            },
        )
        .unwrap();

    assert_eq!(tree.delete_item(child), Some(block(None)));
    assert!(tree.items.get(leaf).is_none());

    assert_eq!(tree.delete_item(child), None);
    assert_eq!(tree.append_item(child, block(None)), None);
    assert_eq!(tree.items.len(), 1);
}

#[test]
fn no_op_and_text_edits_keep_every_object_key() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, (DisplayOuter::Inline, Text("hello".into())));
    let atomic = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let child = ui.create(Node::Text, (Text("body".into()),));

    ui.append(root, text);
    ui.append(root, atomic);
    ui.append(atomic, child);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let original = keys(&tree, key);

    for value in ["hello", "short", "a much longer string"] {
        ui.set_props(text, (Text(value.into()),));

        assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
        assert_eq!(keys(&tree, key), original);

        assert_fresh(&ui, root, &tree, key);
    }
}

#[test]
fn text_property_edits_preserve_leaf_keys_and_repair_invalid_children() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline,));
    let text = ui.create(Node::Text, (Text("before".into()),));

    ui.append(root, text);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let leaf = tree.items.first_child(key).unwrap();
    assert_eq!(tree.items[leaf].ty, LayoutItemTy::Text);

    for inner in [DisplayInner::Flow, DisplayInner::FlowRoot, DisplayInner::Content] {
        ui.set_props(text, (DisplayOuter::Block, inner, Text("after".into())));
        let invalid = tree.append_item(leaf, block(None)).unwrap();

        assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
        assert_eq!(tree.items.first_child(key), Some(leaf));
        assert_eq!(tree.items[leaf].ty, LayoutItemTy::Text);
        assert!(tree.items.get(invalid).is_none());
        assert_eq!(tree.items.len(), 2);
        assert_fresh(&ui, root, &tree, key);

        assert_eq!(builder.build(&ui, text, &mut tree, leaf), Some(leaf));
        assert_eq!(tree.items.parent(leaf), Some(key));
        assert_fresh(&ui, root, &tree, key);
    }
}

#[test]
fn removing_text_preserves_surviving_inline_and_atomic_objects() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline,));
    let text = ui.create(Node::Text, (DisplayOuter::Inline, Text("remove me".into())));
    let atomic = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));

    ui.append(root, text);
    ui.append(root, atomic);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let old: Vec<_> = tree.items.children(key).collect();
    let atomic_keys = keys(&tree, old[1]);

    ui.delete(text);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert!(tree.items.get(old[0]).is_none());
    assert_eq!(keys(&tree, old[1]), atomic_keys);

    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn scoped_updates_reorder_and_prune_without_touching_other_subtrees() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = ui.create(Node::Box, ());
    let b = ui.create(Node::Box, ());
    let c = ui.create(Node::Box, ());

    for node in [a, b, c] {
        ui.append(root, node);
    }

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let original: Vec<_> = tree.items.children(key).collect();

    let outside = tree.add_item(block(None));
    let outside_child = tree.append_item(outside, block(None)).unwrap();

    ui.prepend(root, c);
    ui.delete(b);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        [original[2], original[0]]
    );
    assert!(tree.items.get(original[1]).is_none());
    assert_eq!(tree.items.parent(outside_child), Some(outside));

    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn stale_roots_wrong_types_and_nonleaf_text_are_recovered() {
    let mut ui = Ui::new();
    let source = ui.create(Node::Text, (DisplayOuter::Inline, Text("hello".into())));

    let mut tree = LayoutTree::new();
    let stale = tree.add_item(block(None));
    tree.delete_item(stale);

    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build(&ui, source, &mut tree, stale).unwrap();

    let invalid_child = tree.append_item(key, block(None)).unwrap();
    builder.build(&ui, source, &mut tree, key).unwrap();
    assert!(tree.items.get(invalid_child).is_none());

    assert_fresh(&ui, source, &tree, key);

    let parent = tree.add_item(block(None));
    tree.items.append(parent, key);
    let sibling = tree.append_item(parent, block(None)).unwrap();
    tree.items[key].ty = LayoutItemTy::Inline;

    let replacement = builder.build(&ui, source, &mut tree, key).unwrap();
    assert_ne!(replacement, key);
    assert_eq!(
        tree.items.children(parent).collect::<Vec<_>>(),
        [replacement, sibling]
    );

    assert_eq!(tree.items[replacement].ty, LayoutItemTy::Inline);
    assert_eq!(tree.items[replacement].node, None);
    let leaf = tree.items.first_child(replacement).unwrap();
    assert_fresh(&ui, source, &tree, leaf);

    ui.delete(source);

    assert_eq!(builder.build(&ui, source, &mut tree, replacement), None);
    assert!(tree.items.get(replacement).is_none());
}

#[test]
fn updating_an_inline_block_body_does_not_add_another_wrapper() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let child = ui.create(Node::Text, (Text("inside".into()),));

    ui.append(root, child);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let body = tree.items.first_child(key).unwrap();
    let original = keys(&tree, key);

    assert_eq!(builder.build(&ui, root, &mut tree, body), Some(body));
    assert_eq!(keys(&tree, key), original);

    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn text_wrappers_survive_updates_and_moves_and_are_pruned_on_deletion() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = ui.create(Node::Text, ());
    let b = ui.create(Node::Text, ());
    let block = ui.create(Node::Box, ());

    for child in [a, block, b] {
        ui.append(root, child);
    }

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let top: Vec<_> = tree.items.children(key).collect();
    let original = keys(&tree, key);
    let a_leaf = tree.items.first_child(top[0]).unwrap();

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(keys(&tree, key), original);
    assert_eq!(builder.build(&ui, a, &mut tree, top[0]), Some(top[0]));
    assert_eq!(builder.build(&ui, a, &mut tree, a_leaf), Some(a_leaf));
    assert_eq!(keys(&tree, key), original);

    ui.prepend(root, b);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(tree.items.children(key).collect::<Vec<_>>(), [top[2], top[0], top[1]]);
    assert_fresh(&ui, root, &tree, key);

    ui.delete(a);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert!(tree.items.get(top[0]).is_none());
    assert!(tree.items.get(a_leaf).is_none());
    assert_eq!(tree.items.children(key).collect::<Vec<_>>(), [top[2], top[1]]);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn changing_child_flow_preserves_the_source_container_and_surviving_children() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let inline = ui.create(Node::Text, (DisplayOuter::Inline, Text("inline".into())));
    let block = ui.create(Node::Box, ());

    ui.append(root, inline);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let inline_key = tree.items.first_child(key).unwrap();
    assert_eq!(tree.items[key].ty, LayoutItemTy::Inline);

    ui.append(root, block);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(tree.items[key].ty, LayoutItemTy::Block);
    let wrapper = tree.items.first_child(key).unwrap();
    assert_eq!(tree.items[wrapper].ty, LayoutItemTy::Inline);
    assert_eq!(tree.items[wrapper].node, None);
    assert_eq!(tree.items.first_child(wrapper), Some(inline_key));
    assert_fresh(&ui, root, &tree, key);

    ui.delete(block);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(tree.items[key].ty, LayoutItemTy::Inline);
    assert_eq!(tree.items.first_child(key), Some(inline_key));
    assert!(tree.items.get(wrapper).is_none());
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn anonymous_inline_block_wrappers_follow_their_bodies_during_moves_and_deletion() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let b = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let a_text = ui.create(Node::Text, (Text("a".into()),));
    let b_text = ui.create(Node::Text, (Text("b".into()),));

    ui.append(a, a_text);
    ui.append(b, b_text);
    ui.append(root, a);
    ui.append(root, b);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let wrappers: Vec<_> = tree.items.children(key).collect();
    let a_keys = keys(&tree, wrappers[0]);
    let b_keys = keys(&tree, wrappers[1]);

    ui.prepend(root, b);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        [wrappers[1], wrappers[0]]
    );
    assert_eq!(keys(&tree, wrappers[0]), a_keys);
    assert_eq!(keys(&tree, wrappers[1]), b_keys);
    assert_fresh(&ui, root, &tree, key);

    let body = tree.items.first_child(wrappers[1]).unwrap();
    assert_eq!(tree.items[body].ty, LayoutItemTy::Inline);
    assert_eq!(builder.build(&ui, b, &mut tree, body), Some(body));
    assert_eq!(keys(&tree, wrappers[1]), b_keys);

    ui.delete(a);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert!(a_keys.iter().all(|key| tree.items.get(*key).is_none()));
    assert_eq!(keys(&tree, wrappers[1]), b_keys);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn repeated_display_edits_and_moves_match_fresh_object_trees() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let mut nodes = Vec::new();

    for _ in 0..8 {
        let node = ui.create(Node::Box, (DisplayOuter::Inline,));
        let child = ui.create(Node::Text, (Text("child".into()),));

        ui.append(node, child);
        ui.append(root, node);
        nodes.push(node);
    }

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let mut seed = 7_u32;

    for step in 0..100 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let node = nodes[(seed as usize >> 8) % nodes.len()];

        let outer = if step % 3 == 0 {
            DisplayOuter::Block
        } else {
            DisplayOuter::Inline
        };
        let inner = match step % 4 {
            0 => DisplayInner::FlowRoot,
            1 => DisplayInner::Content,
            _ => DisplayInner::Flow,
        };

        ui.set_props(node, (outer, inner));

        if step % 2 == 0 {
            ui.prepend(root, node);
        } else {
            ui.append(root, node);
        }

        assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));

        assert_fresh(&ui, root, &tree, key);
        assert_eq!(tree.items.len(), keys(&tree, key).len());
    }
}
