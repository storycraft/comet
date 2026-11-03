use comet_layout2::{
    style::bx::{DisplayInner, DisplayOuter},
    tree::{LayoutItemLink, LayoutItemKey, LayoutItemTy, LayoutTree, builder::LayoutTreeBuilder},
};
use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};

fn fragments(tree: &LayoutTree, root: LayoutItemKey, node: NodeKey) -> Vec<LayoutItemKey> {
    let mut pending = vec![root];
    let mut result = Vec::new();

    while let Some(key) = pending.pop() {
        if tree.items[key].ty == LayoutItemTy::Inline(node) {
            result.push(key);
        }

        pending.extend(tree.items.children(key).rev());
    }

    result
}

fn assert_chain(tree: &LayoutTree, keys: &[LayoutItemKey]) {
    for (index, &key) in keys.iter().enumerate() {
        assert_eq!(
            tree.items[key].link,
            LayoutItemLink {
                prev: index.checked_sub(1).map(|index| keys[index]),
                next: keys.get(index + 1).copied(),
            }
        );
    }
}

#[test]
fn nested_inline_fragments_link_across_runs_in_both_directions() {
    for inline_root in [false, true] {
        let mut ui = Ui::new();
        let outer = ui.create(Node::Box, (DisplayOuter::Inline,));
        let inner = ui.create(Node::Box, (DisplayOuter::Inline,));
        ui.append(outer, inner);
        let root = if inline_root {
            outer
        } else {
            let root = ui.create(Node::Box, ());
            ui.append(root, outer);

            root
        };

        // Empty leading and trailing fragments are parts too.
        let first = ui.create(Node::Box, ());
        let text = ui.create(Node::Text, ());
        let last = ui.create(Node::Box, ());

        for node in [first, text, last] {
            ui.append(inner, node);
        }

        let mut tree = LayoutTree::new();
        let mut builder = LayoutTreeBuilder::new();
        let key = builder.build(&ui, root, &mut tree, None).unwrap();
        let outer_parts = fragments(&tree, key, outer);
        let inner_parts = fragments(&tree, key, inner);
        assert_eq!(outer_parts.len(), 3);
        assert_eq!(inner_parts.len(), 3);
        assert_eq!(tree.item_for_node(outer), Some(outer_parts[0]));
        assert_eq!(tree.item_for_node(inner), Some(inner_parts[0]));
        assert_chain(&tree, &outer_parts);
        assert_chain(&tree, &inner_parts);

        for _ in 0..3 {
            assert_eq!(builder.build(&ui, root, &mut tree, Some(key)), Some(key));
            assert_eq!(fragments(&tree, key, outer), outer_parts);
            assert_eq!(fragments(&tree, key, inner), inner_parts);
            assert_eq!(tree.item_for_node(outer), Some(outer_parts[0]));
            assert_eq!(tree.item_for_node(inner), Some(inner_parts[0]));
            assert_chain(&tree, &outer_parts);
            assert_chain(&tree, &inner_parts);
        }

        let mut pending = vec![key];

        while let Some(key) = pending.pop() {
            if !matches!(tree.items[key].ty, LayoutItemTy::Inline(_)) {
                assert_eq!(tree.items[key].link, LayoutItemLink::default());
            }

            pending.extend(tree.items.children(key));
        }
    }
}

#[test]
fn splitting_merging_and_role_changes_reset_continuations() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let text = ui.create(Node::Text, ());
    ui.append(root, span);
    ui.append(span, text);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build(&ui, root, &mut tree, None).unwrap();
    assert_chain(&tree, &fragments(&tree, key, span));

    for _ in 0..3 {
        let block = ui.create(Node::Box, ());
        ui.before(text, block);
        builder.build(&ui, root, &mut tree, Some(key)).unwrap();
        let split = fragments(&tree, key, span);
        assert_eq!(split.len(), 2);
        assert_eq!(tree.item_for_node(span), Some(split[0]));
        assert_chain(&tree, &split);

        ui.delete(block);
        builder.build(&ui, root, &mut tree, Some(key)).unwrap();
        let merged = fragments(&tree, key, span);
        assert_eq!(merged.len(), 1);
        assert_eq!(tree.item_for_node(span), Some(merged[0]));
        assert_chain(&tree, &merged);
        assert!(
            split
                .iter()
                .filter(|&&key| tree.items.get(key).is_some())
                .count()
                <= 1
        );
    }

    let block = ui.create(Node::Box, ());
    ui.before(text, block);
    builder.build(&ui, root, &mut tree, Some(key)).unwrap();
    let split = fragments(&tree, key, span);
    ui.set_props(span, (DisplayInner::FlowRoot,));
    builder.build(&ui, root, &mut tree, Some(key)).unwrap();

    assert!(split.iter().all(|&key| tree.items.get(key).is_none()));
    let atomic = tree.items.first_child(key).unwrap();
    assert_eq!(tree.items[atomic].ty, LayoutItemTy::AtomicInline(span));
    assert_eq!(tree.item_for_node(span), Some(atomic));
    assert_eq!(tree.items[atomic].link, LayoutItemLink::default());
}

#[test]
fn deleting_a_fragment_subtree_reconnects_surviving_parts() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, (DisplayOuter::Inline,));
    let inner = ui.create(Node::Box, (DisplayOuter::Inline,));
    ui.append(root, outer);
    ui.append(outer, inner);

    for kind in [Node::Box, Node::Text, Node::Box] {
        let node = ui.create(kind, ());
        ui.append(inner, node);
    }

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build(&ui, root, &mut tree, None)
        .unwrap();
    let outer_parts = fragments(&tree, key, outer);
    let inner_parts = fragments(&tree, key, inner);
    let middle_run = tree.items.parent(outer_parts[1]).unwrap();
    tree.delete_item(middle_run).unwrap();

    assert!(tree.items.get(outer_parts[1]).is_none());
    assert!(tree.items.get(inner_parts[1]).is_none());
    assert_chain(&tree, &[outer_parts[0], outer_parts[2]]);
    assert_chain(&tree, &[inner_parts[0], inner_parts[2]]);
    assert_eq!(tree.item_for_node(outer), Some(outer_parts[0]));
    assert_eq!(tree.item_for_node(inner), Some(inner_parts[0]));

    tree.delete_item(outer_parts[0]).unwrap();
    assert_chain(&tree, &[outer_parts[2]]);
    assert_chain(&tree, &[inner_parts[2]]);
    assert_eq!(tree.item_for_node(outer), Some(outer_parts[2]));
    assert_eq!(tree.item_for_node(inner), Some(inner_parts[2]));
    assert_eq!(tree.delete_item(outer_parts[0]), None);

    tree.delete_item(outer_parts[2]).unwrap();
    assert_eq!(tree.item_for_node(outer), None);
    assert_eq!(tree.item_for_node(inner), None);
}

#[test]
fn builder_reuse_keeps_independent_builds_separate() {
    let mut ui = Ui::new();
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let block = ui.create(Node::Box, ());
    ui.append(span, block);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let first = builder.build(&ui, span, &mut tree, None).unwrap();
    let first_parts = fragments(&tree, first, span);
    let second = builder.build(&ui, span, &mut tree, None).unwrap();
    let second_parts = fragments(&tree, second, span);
    assert_eq!(first_parts.len(), 2);
    assert_eq!(second_parts.len(), 2);
    assert_chain(&tree, &first_parts);
    assert_chain(&tree, &second_parts);
    assert_eq!(tree.item_for_node(span), Some(second_parts[0]));

    tree.delete_item(first).unwrap();
    assert_eq!(tree.item_for_node(span), Some(second_parts[0]));
}
