use comet_layout2::{
    style::{DisplayInner, DisplayOuter},
    tree::{
        ChildLayoutMode as Mode, LayoutItem, LayoutItemKey, LayoutItemTy as Ty, LayoutTree,
        builder::LayoutTreeBuilder,
    },
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

fn block(node: Option<NodeKey>) -> LayoutItem {
    LayoutItem {
        child_layout: Mode::Block,
        ty: node.map_or(Ty::AnonymousBlock, Ty::Block),
        continuation: Default::default(),
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
        let mut item = tree.items[key];
        item.continuation = Default::default();
        result.push((depth, item));
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

    // Keys belong to their respective arenas; compare fragment positions instead.
    let links = |tree: &LayoutTree, root| {
        let keys = keys(tree, root);
        let positions: std::collections::HashMap<_, _> = keys
            .iter()
            .enumerate()
            .map(|(index, &key)| (key, index))
            .collect();
        let mut seen = std::collections::HashSet::new();

        for &key in &keys {
            if let Some(node) = tree.items[key].ty.node()
                && seen.insert(node)
            {
                assert_eq!(tree.item_for_node(node), Some(key));
            }
        }

        keys.iter()
            .map(|&key| {
                let continuation = tree.items[key].continuation;

                (
                    continuation.prev.map(|key| positions[&key]),
                    continuation.next.map(|key| positions[&key]),
                )
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(links(tree, root), links(&fresh, key));
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
                ty: Ty::AnonymousBlock,
                child_layout: Mode::Inline,
                continuation: Default::default(),
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
    assert_eq!(tree.items[leaf].ty, Ty::Text(text));
    assert_eq!(tree.items[leaf].child_layout, Mode::None);

    for inner in [
        DisplayInner::Flow,
        DisplayInner::FlowRoot,
        DisplayInner::Content,
    ] {
        ui.set_props(text, (DisplayOuter::Block, inner, Text("after".into())));
        let invalid = tree.append_item(leaf, block(None)).unwrap();

        assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
        assert_eq!(tree.items.first_child(key), Some(leaf));
        assert_eq!(tree.items[leaf].ty, Ty::Text(text));
        assert_eq!(tree.items[leaf].child_layout, Mode::None);
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
    tree.items[key].ty = Ty::Inline(source);

    let replacement = builder.build(&ui, source, &mut tree, key).unwrap();
    assert_ne!(replacement, key);
    assert_eq!(
        tree.items.children(parent).collect::<Vec<_>>(),
        [replacement, sibling]
    );

    assert_eq!(tree.items[replacement].child_layout, Mode::Inline);
    assert_eq!(tree.items[replacement].ty, Ty::AnonymousBlock);
    let leaf = tree.items.first_child(replacement).unwrap();
    assert_fresh(&ui, source, &tree, leaf);

    ui.delete(source);

    assert_eq!(builder.build(&ui, source, &mut tree, replacement), None);
    assert!(tree.items.get(replacement).is_none());
}

#[test]
fn updating_an_atomic_inline_preserves_its_direct_contents() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let child = ui.create(Node::Text, (Text("inside".into()),));

    ui.append(root, child);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let original = keys(&tree, key);

    assert_eq!(tree.items[key].ty, Ty::AtomicInline(root));
    assert_eq!(tree.items[key].child_layout, Mode::Inline);
    let leaf = tree.items.first_child(key).unwrap();
    assert_eq!(tree.items[leaf].ty, Ty::Text(child));

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
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
    let b_leaf = tree.items.first_child(top[2]).unwrap();

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(keys(&tree, key), original);
    assert_eq!(builder.build(&ui, a, &mut tree, top[0]), Some(top[0]));
    assert_eq!(builder.build(&ui, a, &mut tree, a_leaf), Some(a_leaf));
    assert_eq!(keys(&tree, key), original);

    ui.prepend(root, b);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        [top[0], top[1]]
    );
    assert_eq!(
        tree.items.children(top[0]).collect::<Vec<_>>(),
        [b_leaf, a_leaf]
    );
    assert!(tree.items.get(top[2]).is_none());
    assert_fresh(&ui, root, &tree, key);

    ui.delete(a);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert!(tree.items.get(a_leaf).is_none());
    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        [top[0], top[1]]
    );
    assert_eq!(tree.items.first_child(top[0]), Some(b_leaf));
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
    assert_eq!(tree.items[key].child_layout, Mode::Inline);

    ui.append(root, block);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(tree.items[key].child_layout, Mode::Block);
    let wrapper = tree.items.first_child(key).unwrap();
    assert_eq!(tree.items[wrapper].child_layout, Mode::Inline);
    assert_eq!(tree.items[wrapper].ty, Ty::AnonymousBlock);
    assert_eq!(tree.items.first_child(wrapper), Some(inline_key));
    assert_fresh(&ui, root, &tree, key);

    ui.delete(block);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(tree.items[key].child_layout, Mode::Inline);
    assert_eq!(tree.items.first_child(key), Some(inline_key));
    assert!(tree.items.get(wrapper).is_none());
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn atomic_inline_boxes_keep_identity_during_moves_and_deletion() {
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
    let atomics: Vec<_> = tree.items.children(key).collect();
    let a_keys = keys(&tree, atomics[0]);
    let b_keys = keys(&tree, atomics[1]);

    for (key, source) in atomics.iter().zip([a, b]) {
        assert_eq!(tree.items[*key].child_layout, Mode::Inline);
        assert_eq!(tree.items[*key].ty, Ty::AtomicInline(source));
    }

    ui.prepend(root, b);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        [atomics[1], atomics[0]]
    );
    assert_eq!(keys(&tree, atomics[0]), a_keys);
    assert_eq!(keys(&tree, atomics[1]), b_keys);
    assert_fresh(&ui, root, &tree, key);

    let atomic = atomics[1];

    assert_eq!(tree.items[atomic].child_layout, Mode::Inline);
    assert_eq!(builder.build(&ui, b, &mut tree, atomic), Some(atomic));
    assert_eq!(keys(&tree, atomics[1]), b_keys);

    ui.delete(a);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert!(a_keys.iter().all(|key| tree.items.get(*key).is_none()));
    assert_eq!(keys(&tree, atomics[1]), b_keys);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn resumed_span_inline_block_and_text_share_one_inline_run() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let before = ui.create(Node::Text, ());
    let interruption = ui.create(Node::Box, ());
    let after = ui.create(Node::Text, ());
    let atomic = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let atomic_text = ui.create(Node::Text, ());
    let ending = ui.create(Node::Text, ());

    ui.append(root, span);
    ui.append(span, before);
    ui.append(span, interruption);
    ui.append(span, after);
    ui.append(root, atomic);
    ui.append(atomic, atomic_text);
    ui.append(root, ending);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let expected = [
        (0, Mode::Block, Ty::Block(root)),
        (1, Mode::Inline, Ty::AnonymousBlock),
        (2, Mode::Inline, Ty::Inline(span)),
        (3, Mode::None, Ty::Text(before)),
        (1, Mode::Block, Ty::Block(interruption)),
        (1, Mode::Inline, Ty::AnonymousBlock),
        (2, Mode::Inline, Ty::Inline(span)),
        (3, Mode::None, Ty::Text(after)),
        (2, Mode::Inline, Ty::AtomicInline(atomic)),
        (3, Mode::None, Ty::Text(atomic_text)),
        (2, Mode::None, Ty::Text(ending)),
    ];
    let actual: Vec<_> = snapshot(&tree, key)
        .into_iter()
        .map(|(depth, item)| (depth, item.child_layout, item.ty))
        .collect();
    assert_eq!(actual, expected);
    let original = keys(&tree, key);

    ui.set_props(after, (Text("updated".into()),));

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(keys(&tree, key), original);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn inserting_a_block_splits_an_inline_run_without_replacing_text_leaves() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = ui.create(Node::Text, ());
    let b = ui.create(Node::Text, ());
    let boundary = ui.create(Node::Box, ());

    for child in [a, b, boundary] {
        ui.append(root, child);
    }

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let run = tree.items.first_child(key).unwrap();
    let leaves: Vec<_> = tree.items.children(run).collect();
    assert_eq!(leaves.len(), 2);

    ui.append(root, b);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    let top: Vec<_> = tree.items.children(key).collect();
    assert_eq!(top.len(), 3);
    assert_eq!(tree.items.first_child(top[0]), Some(leaves[0]));
    assert_eq!(tree.items.first_child(top[2]), Some(leaves[1]));
    assert_fresh(&ui, root, &tree, key);

    ui.delete(boundary);

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(tree.items[key].child_layout, Mode::Inline);
    assert_eq!(tree.items.children(key).collect::<Vec<_>>(), leaves);
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

#[test]
fn semantic_role_changes_replace_items_even_when_child_mode_stays_inline() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let source = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, (Text("content".into()),));

    ui.append(root, source);
    ui.append(source, text);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let root_key = builder.build_new(&ui, root, &mut tree).unwrap();
    let mut previous = tree.items.first_child(root_key).unwrap();

    assert_eq!(tree.items[previous].ty, Ty::Block(source));
    assert_eq!(tree.items[previous].child_layout, Mode::Inline);

    for (outer, inner, expected) in [
        (
            DisplayOuter::Inline,
            DisplayInner::FlowRoot,
            Ty::AtomicInline(source),
        ),
        (DisplayOuter::Inline, DisplayInner::Flow, Ty::Inline(source)),
        (
            DisplayOuter::Block,
            DisplayInner::FlowRoot,
            Ty::Block(source),
        ),
    ] {
        ui.set_props(source, (outer, inner));

        assert_eq!(
            builder.build(&ui, root, &mut tree, root_key),
            Some(root_key)
        );

        let current = tree.items.first_child(root_key).unwrap();

        assert_ne!(current, previous);
        assert!(tree.items.get(previous).is_none());
        assert_eq!(tree.items[current].ty, expected);
        assert_eq!(tree.items[current].child_layout, Mode::Inline);
        assert_eq!(tree.items.len(), 3);
        assert_fresh(&ui, root, &tree, root_key);

        previous = current;
    }
}

#[test]
fn content_mode_prunes_children_and_restores_flow() {
    for outer in [DisplayOuter::Block, DisplayOuter::Inline] {
        let mut ui = Ui::new();
        let root = ui.create(Node::Box, ());
        let source = ui.create(Node::Box, (outer, DisplayInner::FlowRoot));
        let text = ui.create(Node::Text, ());
        let block = ui.create(Node::Box, ());

        ui.append(root, source);
        ui.append(source, text);
        ui.append(source, block);

        let mut tree = LayoutTree::new();
        let mut builder = LayoutTreeBuilder::new();
        let key = builder.build_new(&ui, root, &mut tree).unwrap();
        let child = tree.items.first_child(key).unwrap();
        let role = tree.items[child].ty;

        for mode in [Mode::None, Mode::Block, Mode::None] {
            let inner = if mode == Mode::None {
                DisplayInner::Content
            } else {
                DisplayInner::FlowRoot
            };
            let previous = keys(&tree, child);
            ui.set_props(source, (outer, inner));

            assert_eq!(builder.build(&ui, source, &mut tree, child), Some(child));
            assert_eq!(tree.items[child].ty, role);
            assert_eq!(tree.items[child].child_layout, mode);
            assert_fresh(&ui, root, &tree, key);

            if mode == Mode::None {
                assert!(tree.items.first_child(child).is_none());
                assert!(
                    previous[1..]
                        .iter()
                        .all(|&key| tree.items.get(key).is_none())
                );
            } else {
                assert_eq!(tree.items.children(child).count(), 2);
            }
        }
    }
}

#[test]
fn wide_reorders_and_replacements_preserve_surviving_keys() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let nodes: Vec<_> = (0..256)
        .map(|_| {
            let node = ui.create(Node::Box, ());
            ui.append(root, node);

            node
        })
        .collect();
    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let original: Vec<_> = tree.items.children(key).collect();

    for &node in &nodes {
        ui.prepend(root, node);
    }

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        original.iter().rev().copied().collect::<Vec<_>>()
    );

    for (index, &node) in nodes.iter().enumerate() {
        if index % 2 == 0 {
            let replacement = ui.create(Node::Box, ());
            ui.before(node, replacement);
            ui.delete(node);
        }
    }

    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));

    for (index, &old) in original.iter().enumerate() {
        assert_eq!(tree.items.get(old).is_some(), index % 2 != 0);
    }

    assert_fresh(&ui, root, &tree, key);
    assert_eq!(tree.items.len(), nodes.len() + 1);
}

#[test]
fn many_run_splits_merges_and_reorders_preserve_text_keys() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let mut texts = Vec::new();

    for _ in 0..128 {
        let text = ui.create(Node::Text, ());
        ui.append(root, text);
        texts.push(text);
    }

    // Keep block flow throughout so updates also exercise promotion from old runs.
    let end = ui.create(Node::Box, ());
    ui.append(root, end);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let run = tree.items.first_child(key).unwrap();
    let leaves: Vec<_> = tree.items.children(run).collect();

    for _ in 0..3 {
        let mut blocks = Vec::new();

        for &text in &texts {
            let block = ui.create(Node::Box, ());
            ui.after(text, block);
            blocks.push(block);
        }

        for phase in 0..3 {
            if phase == 1 {
                for &text in &texts {
                    ui.prepend(root, text);
                }
            } else if phase == 2 {
                for &block in &blocks {
                    ui.delete(block);
                }
            }

            assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
            assert_fresh(&ui, root, &tree, key);
            assert_eq!(tree.items.len(), keys(&tree, key).len());

            for (&leaf, &text) in leaves.iter().zip(&texts) {
                assert_eq!(tree.items[leaf].ty, Ty::Text(text));
            }
        }
    }
}

#[test]
fn nested_fragment_reconciliation_matches_fresh_trees() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let mut spans = Vec::new();
    let mut nodes = Vec::new();

    for _ in 0..12 {
        let outer = ui.create(Node::Box, (DisplayOuter::Inline,));
        let inner = ui.create(Node::Box, (DisplayOuter::Inline,));
        ui.append(root, outer);
        ui.append(outer, inner);
        spans.push(outer);

        for _ in 0..8 {
            let node = ui.create(Node::Box, (DisplayOuter::Inline,));
            let text = ui.create(Node::Text, ());
            ui.append(node, text);
            ui.append(inner, node);
            nodes.push((inner, node));
        }
    }

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let mut seed = 17_u32;

    for step in 0..512 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let (parent, node) = nodes[(seed as usize >> 8) % nodes.len()];
        let outer = if step % 3 == 0 {
            DisplayOuter::Block
        } else {
            DisplayOuter::Inline
        };
        let inner = match step % 5 {
            0 => DisplayInner::FlowRoot,
            1 => DisplayInner::Content,
            _ => DisplayInner::Flow,
        };
        ui.set_props(node, (outer, inner));
        ui.prepend(parent, node);
        ui.prepend(root, spans[(seed as usize >> 16) % spans.len()]);

        assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
        assert_fresh(&ui, root, &tree, key);
        assert_eq!(tree.items.len(), keys(&tree, key).len());
    }
}
