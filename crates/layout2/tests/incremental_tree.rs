use comet_layout2::{
    style::{DisplayInner, DisplayOuter},
    tree::{
        InlineIns, InlineLayoutItem, LayoutItem, LayoutItemKey, LayoutItemTy, LayoutTree,
        builder::LayoutTreeBuilder,
    },
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

fn text(ui: &mut Ui, value: &str) -> NodeKey {
    ui.create(Node::Text, (Text(value.into()), DisplayOuter::Inline))
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

fn snapshot(tree: &LayoutTree, root: LayoutItemKey) -> Vec<String> {
    let order = keys(tree, root);
    order
        .iter()
        .map(|&key| {
            let item = &tree.items[key];
            let children: Vec<_> = tree
                .items
                .children(key)
                .map(|child| order.iter().position(|&key| key == child).unwrap())
                .collect();
            match &item.ty {
                LayoutItemTy::Block => format!("{:?} Block {children:?}", item.node),
                LayoutItemTy::Inline(inline) => {
                    let instructions: Vec<_> = inline
                        .instructions
                        .iter()
                        .map(|ins| match ins {
                            InlineIns::InlineBlock(child) => format!(
                                "Box {}",
                                order.iter().position(|key| key == child).unwrap()
                            ),
                            _ => format!("{ins:?}"),
                        })
                        .collect();
                    format!(
                        "{:?} Inline {:?} {instructions:?} {children:?}",
                        item.node, inline.text
                    )
                }
            }
        })
        .collect()
}

fn assert_fresh(ui: &Ui, source: NodeKey, tree: &LayoutTree, root: LayoutItemKey) {
    let mut fresh = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(ui, source, &mut fresh)
        .unwrap();
    assert_eq!(snapshot(tree, root), snapshot(&fresh, key));
}

#[test]
fn public_item_api_owns_children_and_removes_inline_references() {
    let mut tree = LayoutTree::new();
    let root = tree.add_item(LayoutItem {
        ty: LayoutItemTy::Inline(InlineLayoutItem::default()),
        node: None,
    });
    let child = tree.append_item(root, block(None)).unwrap();
    let grandchild = tree.append_item(child, block(None)).unwrap();
    if let LayoutItemTy::Inline(inline) = &mut tree.items[root].ty {
        inline.instructions.push(InlineIns::InlineBlock(child));
    }

    assert_eq!(tree.delete_item(child), Some(block(None)));
    assert!(tree.items.get(grandchild).is_none());
    assert_eq!(tree.items.len(), 1);
    assert_eq!(
        tree.items[root].ty,
        LayoutItemTy::Inline(InlineLayoutItem::default())
    );
    assert_eq!(tree.delete_item(child), None);
    assert_eq!(tree.append_item(child, block(None)), None);
    assert_eq!(tree.items.len(), 1);
}

#[test]
fn no_op_build_reuses_every_key_and_atomic_reference() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let label = text(&mut ui, "hello");
    let atomic = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let inside = text(&mut ui, "inside");
    ui.append(root, label);
    ui.append(root, atomic);
    ui.append(atomic, inside);

    let mut tree = LayoutTree::new();
    // Updating must not disturb preexisting items outside the requested root.
    tree.add_item(block(None));
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let original = keys(&tree, key);
    for _ in 0..3 {
        assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
        assert_eq!(keys(&tree, key), original);
        assert_fresh(&ui, root, &tree, key);
    }
}

#[test]
fn edits_reorder_reuse_and_delete_only_within_the_requested_boundary() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = ui.create(Node::Box, ());
    let b = ui.create(Node::Box, ());
    let c = ui.create(Node::Box, ());
    let label = text(&mut ui, "old");
    for child in [a, b, c] {
        ui.append(root, child);
    }
    ui.append(a, label);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let original: Vec<_> = tree.items.children(key).collect();
    let a_run = tree.items.first_child(original[0]).unwrap();
    let outside = tree.add_item(block(None));
    let outside_child = tree.append_item(outside, block(None)).unwrap();

    ui.set_props(label, (Text("changed".into()),));
    assert_eq!(
        builder.build(&ui, a, &mut tree, original[0]),
        Some(original[0])
    );
    assert_eq!(tree.items.first_child(original[0]), Some(a_run));
    assert_eq!(tree.items.children(key).collect::<Vec<_>>(), original);
    assert_fresh(&ui, root, &tree, key);

    ui.prepend(root, c);
    ui.delete(b);
    let added = ui.create(Node::Box, ());
    ui.append(root, added);
    assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
    let children: Vec<_> = tree.items.children(key).collect();
    assert_eq!(children[0], original[2]);
    assert_eq!(children[1], original[0]);
    assert!(tree.items.get(original[1]).is_none());
    assert_eq!(tree.items.parent(outside_child), Some(outside));
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn stale_target_and_deleted_source_are_recoverable() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let mut tree = LayoutTree::new();
    let target = tree.add_item(block(Some(root)));
    tree.delete_item(target);
    let outside = tree.add_item(block(None));

    let mut builder = LayoutTreeBuilder::new();
    let replacement = builder.build(&ui, root, &mut tree, target).unwrap();
    assert_ne!(replacement, target);
    assert_fresh(&ui, root, &tree, replacement);
    ui.delete(root);
    assert_eq!(builder.build(&ui, root, &mut tree, replacement), None);
    assert!(tree.items.get(replacement).is_none());
    assert_eq!(tree.items.get(outside), Some(&block(None)));
}

#[test]
fn malformed_inline_branches_are_discarded_without_following_foreign_references() {
    for corruption in 0..5 {
        let mut ui = Ui::new();
        let root = ui.create(Node::Box, ());
        let label = text(&mut ui, "가");
        ui.append(root, label);
        let mut tree = LayoutTree::new();
        let mut builder = LayoutTreeBuilder::new();
        let key = builder.build_new(&ui, root, &mut tree).unwrap();
        let run = tree.items.first_child(key).unwrap();
        let foreign = tree.add_item(block(None));
        let stale = tree.add_item(block(None));
        tree.delete_item(stale);
        let LayoutItemTy::Inline(inline) = &mut tree.items[run].ty else {
            unreachable!()
        };
        match corruption {
            0 => inline.instructions = vec![InlineIns::PopInline],
            1 => {
                inline.instructions = vec![InlineIns::Text {
                    node: label,
                    range: 0..1,
                }]
            }
            2 => inline.instructions.push(InlineIns::InlineBlock(stale)),
            3 => inline.instructions.push(InlineIns::InlineBlock(foreign)),
            _ => inline.instructions.push(InlineIns::PushInline(label)),
        }

        assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
        assert!(tree.items.get(run).is_none());
        assert!(tree.items.get(foreign).is_some());
        assert_fresh(&ui, root, &tree, key);
    }
}

#[test]
fn malformed_target_is_replaced_in_its_original_sibling_position() {
    let mut ui = Ui::new();
    let source = text(&mut ui, "ok");
    let mut tree = LayoutTree::new();
    let parent = tree.add_item(block(None));
    let left = tree.append_item(parent, block(None)).unwrap();
    let mut builder = LayoutTreeBuilder::new();
    let target = builder.build_new(&ui, source, &mut tree).unwrap();
    tree.items.append(parent, target);
    let right = tree.append_item(parent, block(None)).unwrap();
    if let LayoutItemTy::Inline(inline) = &mut tree.items[target].ty {
        inline.text.clear();
    }

    let replacement = builder.build(&ui, source, &mut tree, target).unwrap();
    assert_ne!(replacement, target);
    assert!(tree.items.get(target).is_none());
    assert_eq!(
        tree.items.children(parent).collect::<Vec<_>>(),
        [left, replacement, right]
    );
    assert_fresh(&ui, source, &tree, replacement);
}

#[test]
fn atomic_body_updates_keep_the_owner_reference_and_recover_wrong_item_types() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let atomic = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let label = text(&mut ui, "old");
    ui.append(root, atomic);
    ui.append(atomic, label);
    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let run = tree.items.first_child(key).unwrap();
    let body = tree.items.first_child(run).unwrap();
    ui.set_props(label, (Text("new".into()),));
    assert_eq!(builder.build(&ui, atomic, &mut tree, body), Some(body));
    assert_fresh(&ui, root, &tree, key);

    tree.items[body].ty = LayoutItemTy::Inline(InlineLayoutItem::default());
    let replacement = builder.build(&ui, atomic, &mut tree, body).unwrap();
    assert_ne!(replacement, body);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn display_changes_at_a_flow_boundary_match_a_fresh_build() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let child = ui.create(Node::Box, ());
    let label = text(&mut ui, "inside");
    ui.append(root, span);
    ui.append(span, child);
    ui.append(child, label);
    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();

    for inner in [
        DisplayInner::FlowRoot,
        DisplayInner::Content,
        DisplayInner::Flow,
    ] {
        ui.set_props(span, (inner,));
        assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
        assert_fresh(&ui, root, &tree, key);
    }
    ui.set_props(child, (DisplayOuter::Inline,));
    builder.build(&ui, root, &mut tree, key).unwrap();
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn repeated_reordering_and_display_changes_match_fresh_builds() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let mut sources = Vec::new();
    for i in 0..12 {
        let node = ui.create(Node::Box, ());
        let label = text(&mut ui, &format!("node {i}"));
        ui.append(node, label);
        ui.append(root, node);
        sources.push((node, label));
    }

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    let mut seed = 7_u32;

    for step in 0..100 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let index = (seed as usize >> 8) % sources.len();
        let (node, label) = sources[index];
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
        ui.set_props(label, (Text(format!("edit {step}")),));
        if step % 2 == 0 {
            ui.prepend(root, node);
        } else {
            ui.append(root, node);
        }

        assert_eq!(builder.build(&ui, root, &mut tree, key), Some(key));
        assert_fresh(&ui, root, &tree, key);
        // No unowned leftovers may accumulate as inline groups split and merge.
        assert_eq!(tree.items.len(), keys(&tree, key).len());
    }
}
