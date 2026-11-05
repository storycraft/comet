use comet_layout::{
    style::bx::{DisplayInner, DisplayOuter},
    tree::{
        LayoutItemFlags as Flags, LayoutItemKey, LayoutItemTy, LayoutTree,
        builder::LayoutTreeBuilder,
    },
};
use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};

fn assert_clean(tree: &LayoutTree, root: LayoutItemKey) {
    for key in tree.items.descendants(root) {
        assert!(tree.items[key].flags.is_empty(), "dirty item {key:?}");
    }
}

fn assert_fresh(ui: &Ui, source: NodeKey, tree: &LayoutTree, root: LayoutItemKey) {
    let mut fresh = LayoutTree::new();
    let fresh_root = LayoutTreeBuilder::new()
        .build(ui, source, &mut fresh, None)
        .unwrap();
    let snapshot = |tree: &LayoutTree, root| {
        let keys: Vec<_> = tree.items.descendants(root).collect();
        let position = |key: Option<LayoutItemKey>| {
            key.map(|key| keys.iter().position(|&item| item == key).unwrap())
        };

        keys.iter()
            .map(|&key| {
                let item = tree.items[key];

                (
                    item.ty,
                    item.child_layout,
                    item.flags,
                    position(tree.items.parent(key)),
                    position(item.link.prev),
                    position(item.link.next),
                )
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(snapshot(tree, root), snapshot(&fresh, fresh_root));
}

#[test]
fn flags_propagate_and_clean_subtrees_are_not_read() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let left = ui.create(Node::Box, (DisplayInner::Flow,));
    let right = ui.create(Node::Box, (DisplayInner::Flow, DisplayOuter::Block));
    let leaf = ui.create(Node::Text, ());

    ui.append(root, left);
    ui.append(root, right);
    ui.append(left, leaf);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build(&ui, root, &mut tree, None).unwrap();
    let left_key = tree.item_for_node(left).unwrap();
    let right_key = tree.item_for_node(right).unwrap();
    let leaf_key = tree.item_for_node(leaf).unwrap();

    assert_clean(&tree, key);
    tree.mark_dirty(leaf_key);
    tree.mark_dirty(leaf_key);

    assert_eq!(tree.items[leaf_key].flags, Flags::DIRTY);
    assert_eq!(tree.items[left_key].flags, Flags::CHILDREN_DIRTY);
    assert_eq!(tree.items[key].flags, Flags::CHILDREN_DIRTY);
    assert!(tree.items[right_key].flags.is_empty());

    // A mutable property borrow would panic if the builder inspected this branch.
    let borrowed = ui.prop_mut::<DisplayInner>(right).unwrap();

    assert_eq!(builder.build(&ui, root, &mut tree, Some(key)), Some(key));
    drop(borrowed);
    assert_clean(&tree, key);
    assert_fresh(&ui, root, &tree, key);

    let borrowed = ui.prop_mut::<DisplayInner>(left).unwrap();

    assert_eq!(builder.build(&ui, root, &mut tree, Some(key)), Some(key));
    drop(borrowed);
}

#[test]
fn source_marking_covers_all_fragments_and_only_the_mapped_build() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let boundary = ui.create(Node::Box, ());

    ui.append(root, span);
    ui.append(span, boundary);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let first = builder.build(&ui, root, &mut tree, None).unwrap();
    let second = builder.build(&ui, root, &mut tree, None).unwrap();

    tree.mark_node_dirty(span);
    assert_clean(&tree, first);

    let parts: Vec<_> = tree
        .items
        .descendants(second)
        .filter(|&key| tree.items[key].ty == LayoutItemTy::Inline(span))
        .collect();

    assert_eq!(parts.len(), 2);

    for &part in &parts {
        assert_eq!(tree.items[part].flags, Flags::DIRTY);
        assert!(
            tree.items[tree.items.parent(part).unwrap()]
                .flags
                .contains(Flags::CHILDREN_DIRTY)
        );
    }

    ui.set_props(span, (DisplayInner::FlowRoot,));
    builder.build(&ui, root, &mut tree, Some(second)).unwrap();
    assert_clean(&tree, second);
    assert_clean(&tree, first);
    assert_fresh(&ui, root, &tree, second);
}

#[test]
fn structural_edits_and_stale_marks_reconcile_from_the_root() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let left = ui.create(Node::Box, ());
    let right = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, ());

    ui.append(root, left);
    ui.append(root, right);
    ui.append(left, text);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build(&ui, root, &mut tree, None).unwrap();

    ui.append(right, text);
    tree.mark_node_dirty(left);
    tree.mark_node_dirty(right);
    builder.build(&ui, root, &mut tree, Some(key)).unwrap();
    assert_fresh(&ui, root, &tree, key);

    let stale = tree.item_for_node(text).unwrap();

    ui.delete(text);
    tree.mark_node_dirty(text);
    builder.build(&ui, root, &mut tree, Some(key)).unwrap();
    assert!(tree.items.get(stale).is_none());
    tree.mark_dirty(stale);
    tree.mark_node_dirty(text);
    assert_clean(&tree, key);
    assert_fresh(&ui, root, &tree, key);

    let inserted = ui.create(Node::Text, ());

    ui.append(left, inserted);
    tree.mark_node_dirty(left);
    builder.build(&ui, root, &mut tree, Some(key)).unwrap();
    assert_fresh(&ui, root, &tree, key);

    tree.mark_dirty(key);
    ui.delete(root);
    assert_eq!(builder.build(&ui, root, &mut tree, Some(key)), None);
    assert!(tree.items.is_empty());
}

#[test]
fn repeated_flagged_edits_match_fresh_builds() {
    let mut random = 873_u64;
    let mut next = |limit: usize| {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);

        ((random >> 32) as usize) % limit
    };

    for _ in 0..20 {
        let mut ui = Ui::new();
        let nodes: Vec<_> = (0..30)
            .map(|i| ui.create(if i > 20 { Node::Text } else { Node::Box }, ()))
            .collect();

        for i in 1..nodes.len() {
            ui.append(nodes[next(i.min(21))], nodes[i]);
        }

        let mut tree = LayoutTree::new();
        let mut builder = LayoutTreeBuilder::new();
        let mut root = builder.build(&ui, nodes[0], &mut tree, None).unwrap();

        for _ in 0..100 {
            let i = next(nodes.len());

            if tree.item_for_node(nodes[i]).is_none() {
                continue;
            }

            tree.mark_node_dirty(nodes[i]);

            if next(4) == 0 && i != 0 {
                let parent = nodes[next(21)];

                if let Some(old_parent) = ui.parent(nodes[i]) {
                    tree.mark_node_dirty(old_parent);
                }

                tree.mark_node_dirty(parent);
                ui.append(parent, nodes[i]);
            } else {
                let outer = if next(2) == 0 {
                    DisplayOuter::Inline
                } else {
                    DisplayOuter::Block
                };
                let inner = match next(3) {
                    0 => DisplayInner::Flow,
                    1 => DisplayInner::FlowRoot,
                    _ => DisplayInner::Content,
                };

                ui.set_props(nodes[i], (outer, inner));
            }

            root = builder.build(&ui, nodes[0], &mut tree, Some(root)).unwrap();
            assert_fresh(&ui, nodes[0], &tree, root);
            assert_clean(&tree, root);
        }
    }
}

#[test]
fn clean_subtree_returns_without_resolving_dirty_ancestors() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayInner::Flow,));
    let child = ui.create(Node::Box, ());

    ui.append(root, child);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build(&ui, root, &mut tree, None).unwrap();
    let child_key = tree.item_for_node(child).unwrap();

    tree.mark_dirty(key);

    let borrowed = ui.prop_mut::<DisplayInner>(root).unwrap();

    assert_eq!(
        builder.build(&ui, child, &mut tree, Some(child_key)),
        Some(child_key)
    );
    assert_eq!(tree.items[key].flags, Flags::DIRTY);
    assert_clean(&tree, child_key);
    drop(borrowed);
}
