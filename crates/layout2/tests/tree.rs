use comet_layout2::{
    style::{DisplayInner, DisplayOuter},
    tree::{
        InlineIns, InlineLayoutItem, LayoutItemKey, LayoutItemTy, LayoutTree,
        builder::LayoutTreeBuilder,
    },
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

fn text(ui: &mut Ui, value: &str) -> NodeKey {
    ui.create(Node::Text, (Text(value.into()), DisplayOuter::Inline))
}

fn content(tree: &LayoutTree, key: LayoutItemKey) -> &InlineLayoutItem {
    let LayoutItemTy::Inline(content) = &tree.items[key].ty else {
        panic!("expected inline item");
    };

    content
}

#[test]
fn builds_only_the_requested_subtree_and_retains_existing_items() {
    let mut ui = Ui::new();
    let parent = ui.create(Node::Box, ());
    let root = ui.create(Node::Box, ());
    let sibling = ui.create(Node::Box, ());
    let child = ui.create(Node::Box, ());
    ui.append(parent, root);
    ui.append(parent, sibling);
    ui.append(root, child);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let first = builder.build_new(&ui, root, &mut tree).unwrap();
    assert_eq!(tree.items[first].node, Some(root));
    assert_eq!(tree.items[first].ty, LayoutItemTy::Block);
    let children: Vec<_> = tree.items.children(first).collect();
    assert_eq!(children.len(), 1);
    assert_eq!(tree.items[children[0]].node, Some(child));
    assert_eq!(tree.items.len(), 2);

    let second = builder.build_new(&ui, sibling, &mut tree).unwrap();
    assert_ne!(first, second);
    assert_eq!(tree.items.len(), 3);
    assert_eq!(tree.items.parent(first), None);
    assert_eq!(tree.items.parent(second), None);

    ui.delete(sibling);
    assert_eq!(builder.build_new(&ui, sibling, &mut tree), None);
    assert_eq!(tree.items.len(), 3);
}

#[test]
fn merges_adjacent_and_nested_inline_elements_with_utf8_source_ranges() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = text(&mut ui, "가");
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let b = text(&mut ui, "🙂 ");
    ui.append(root, a);
    ui.append(root, span);
    ui.append(span, b);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    let children: Vec<_> = tree.items.children(key).collect();
    assert_eq!(children.len(), 1);
    let run = children[0];
    assert_eq!(tree.items[run].node, None);
    assert_eq!(tree.items.len(), 2);
    assert_eq!(content(&tree, run).text, "가🙂 ");
    assert_eq!(
        content(&tree, run).instructions,
        vec![
            InlineIns::PushInline(a),
            InlineIns::Text {
                node: a,
                range: 0..3
            },
            InlineIns::PopInline,
            InlineIns::PushInline(span),
            InlineIns::PushInline(b),
            InlineIns::Text {
                node: b,
                range: 3..8
            },
            InlineIns::PopInline,
            InlineIns::PopInline,
        ]
    );
}

#[test]
fn blocks_separate_inline_runs_in_source_order() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = text(&mut ui, "before");
    let block = ui.create(Node::Box, (DisplayOuter::Block,));
    let b = text(&mut ui, "after");
    for child in [a, block, b] {
        ui.append(root, child);
    }

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    let children: Vec<_> = tree.items.children(key).collect();
    assert_eq!(children.len(), 3);
    assert_eq!(content(&tree, children[0]).text, "before");
    assert_eq!(tree.items[children[1]].ty, LayoutItemTy::Block);
    assert_eq!(tree.items[children[1]].node, Some(block));
    assert_eq!(content(&tree, children[2]).text, "after");
}

#[test]
fn inline_root_splits_around_a_block_with_an_anonymous_container() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline,));
    let a = text(&mut ui, "a");
    let block = ui.create(Node::Box, ());
    let inner = text(&mut ui, "inside");
    let b = text(&mut ui, "b");
    for child in [a, block, b] {
        ui.append(root, child);
    }
    ui.append(block, inner);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    assert_eq!(tree.items[key].node, None);
    assert_eq!(tree.items[key].ty, LayoutItemTy::Block);
    let children: Vec<_> = tree.items.children(key).collect();
    assert_eq!(children.len(), 3);
    let block_key = children[1];
    assert_eq!(tree.items[block_key].node, Some(block));
    assert_eq!(tree.items[block_key].ty, LayoutItemTy::Block);
    assert_eq!(content(&tree, children[0]).text, "a");
    assert_eq!(content(&tree, children[2]).text, "b");
    assert_eq!(
        content(&tree, children[0]).instructions,
        vec![
            InlineIns::PushInline(root),
            InlineIns::PushInline(a),
            InlineIns::Text {
                node: a,
                range: 0..1
            },
            InlineIns::PopInline,
            InlineIns::PopInline,
        ]
    );
    assert_eq!(
        content(&tree, children[2]).instructions,
        vec![
            InlineIns::PushInline(root),
            InlineIns::PushInline(b),
            InlineIns::Text {
                node: b,
                range: 0..1
            },
            InlineIns::PopInline,
            InlineIns::PopInline,
        ]
    );
    let inner_key = tree.items.first_child(block_key).unwrap();
    assert_eq!(content(&tree, inner_key).text, "inside");

    tree.items.delete_node(key);
    assert!(tree.items.is_empty());
}

#[test]
fn nested_inline_spans_reopen_after_blocks_and_close_before_following_siblings() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, (DisplayOuter::Inline,));
    let inner = ui.create(Node::Box, (DisplayOuter::Inline,));
    let a = text(&mut ui, "가");
    let block = ui.create(Node::Box, ());
    let b = text(&mut ui, "🙂");
    let c = text(&mut ui, "c");
    let d = text(&mut ui, "d");
    ui.append(root, outer);
    ui.append(root, d);
    ui.append(outer, inner);
    ui.append(outer, c);
    for child in [a, block, b] {
        ui.append(inner, child);
    }

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    let children: Vec<_> = tree.items.children(key).collect();
    assert_eq!(children.len(), 3);
    assert_eq!(tree.items[children[1]].node, Some(block));
    assert_eq!(content(&tree, children[0]).text, "가");
    assert_eq!(content(&tree, children[2]).text, "🙂cd");
    assert_eq!(
        content(&tree, children[0]).instructions,
        vec![
            InlineIns::PushInline(outer),
            InlineIns::PushInline(inner),
            InlineIns::PushInline(a),
            InlineIns::Text {
                node: a,
                range: 0..3
            },
            InlineIns::PopInline,
            InlineIns::PopInline,
            InlineIns::PopInline,
        ]
    );
    assert_eq!(
        content(&tree, children[2]).instructions,
        vec![
            InlineIns::PushInline(outer),
            InlineIns::PushInline(inner),
            InlineIns::PushInline(b),
            InlineIns::Text {
                node: b,
                range: 0..4
            },
            InlineIns::PopInline,
            InlineIns::PopInline,
            InlineIns::PushInline(c),
            InlineIns::Text {
                node: c,
                range: 4..5
            },
            InlineIns::PopInline,
            InlineIns::PopInline,
            InlineIns::PushInline(d),
            InlineIns::Text {
                node: d,
                range: 5..6
            },
            InlineIns::PopInline,
        ]
    );
}

#[test]
fn edge_fragments_are_preserved_without_separating_consecutive_blocks() {
    for position in 0..3 {
        let mut ui = Ui::new();
        let root = ui.create(Node::Box, ());
        let span = ui.create(Node::Box, (DisplayOuter::Inline,));
        let first = ui.create(Node::Box, ());
        let second = ui.create(Node::Box, ());
        let label = text(&mut ui, "text");
        ui.append(root, span);
        let mut source = vec![first, second];
        source.insert(position, label);
        for &child in &source {
            ui.append(span, child);
        }

        let mut tree = LayoutTree::new();
        let key = LayoutTreeBuilder::new()
            .build_new(&ui, root, &mut tree)
            .unwrap();
        let children: Vec<_> = tree.items.children(key).collect();
        let leading = usize::from(position != 0);
        let trailing = usize::from(position != 2);
        assert_eq!(children.len(), 3 + leading + trailing);
        for &index in [
            (leading == 1).then_some(0),
            (trailing == 1).then_some(children.len() - 1),
        ]
        .iter()
        .flatten()
        {
            let fragment = content(&tree, children[index]);
            assert_eq!(fragment.text, "");
            assert_eq!(
                fragment.instructions,
                vec![InlineIns::PushInline(span), InlineIns::PopInline,]
            );
        }
        for (&child, &node) in children[leading..leading + 3].iter().zip(&source) {
            if node == label {
                assert_eq!(content(&tree, child).text, "text");
                assert_eq!(
                    content(&tree, child).instructions.first(),
                    Some(&InlineIns::PushInline(span))
                );
                assert_eq!(
                    content(&tree, child).instructions.last(),
                    Some(&InlineIns::PopInline)
                );
            } else {
                assert_eq!(tree.items[child].node, Some(node));
                assert_eq!(tree.items[child].ty, LayoutItemTy::Block);
            }
        }
    }
}

#[test]
fn inline_root_with_only_a_block_keeps_a_wrapper_and_empty_inline_is_preserved() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline,));
    let block = ui.create(Node::Box, ());
    ui.append(root, block);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    assert_eq!(tree.items[key].node, None);
    assert_eq!(tree.items[key].ty, LayoutItemTy::Block);
    let children: Vec<_> = tree.items.children(key).collect();
    assert_eq!(children.len(), 3);
    assert_eq!(tree.items[children[1]].node, Some(block));
    for &fragment in &[children[0], children[2]] {
        assert_eq!(content(&tree, fragment).text, "");
        assert_eq!(
            content(&tree, fragment).instructions,
            vec![InlineIns::PushInline(root), InlineIns::PopInline,]
        );
    }

    let empty = ui.create(Node::Box, (DisplayOuter::Inline,));
    let empty_key = LayoutTreeBuilder::new()
        .build_new(&ui, empty, &mut tree)
        .unwrap();
    assert_eq!(tree.items[empty_key].node, Some(empty));
    assert_eq!(
        content(&tree, empty_key).instructions,
        vec![InlineIns::PushInline(empty), InlineIns::PopInline,]
    );
}

#[test]
fn text_nodes_honor_display_and_missing_text_is_empty() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Text, (Text("block text".into()),));
    let empty = ui.create(Node::Text, (DisplayOuter::Inline,));
    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();

    let key = builder.build_new(&ui, root, &mut tree).unwrap();
    assert_eq!(tree.items[key].ty, LayoutItemTy::Block);
    let child = tree.items.first_child(key).unwrap();
    assert_eq!(content(&tree, child).text, "block text");
    assert_eq!(
        content(&tree, child).instructions,
        vec![InlineIns::Text {
            node: root,
            range: 0..10
        },]
    );

    let empty_key = builder.build_new(&ui, empty, &mut tree).unwrap();
    assert_eq!(content(&tree, empty_key).text, "");
    assert_eq!(
        content(&tree, empty_key).instructions,
        vec![
            InlineIns::PushInline(empty),
            InlineIns::Text {
                node: empty,
                range: 0..0
            },
            InlineIns::PopInline,
        ]
    );

    tree.clear();
    assert!(tree.items.is_empty());
}

#[test]
fn nested_inline_ancestors_keep_empty_fragments_around_a_block_group() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, (DisplayOuter::Inline,));
    let inner = ui.create(Node::Box, (DisplayOuter::Inline,));
    let first = ui.create(Node::Box, ());
    let second = ui.create(Node::Box, ());
    ui.append(root, outer);
    ui.append(outer, inner);
    ui.append(inner, first);
    ui.append(inner, second);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    let children: Vec<_> = tree.items.children(key).collect();
    assert_eq!(children.len(), 4);
    assert_eq!(tree.items[children[1]].node, Some(first));
    assert_eq!(tree.items[children[2]].node, Some(second));
    for &fragment in &[children[0], children[3]] {
        assert_eq!(tree.items[fragment].node, None);
        assert_eq!(content(&tree, fragment).text, "");
        assert_eq!(
            content(&tree, fragment).instructions,
            vec![
                InlineIns::PushInline(outer),
                InlineIns::PushInline(inner),
                InlineIns::PopInline,
                InlineIns::PopInline,
            ]
        );
    }
}

#[test]
fn intervening_empty_inline_ends_a_block_group_and_retains_all_span_tags() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline,));
    let first = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let second = ui.create(Node::Box, ());
    for child in [first, span, second] {
        ui.append(root, child);
    }

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    let children: Vec<_> = tree.items.children(key).collect();
    assert_eq!(children.len(), 5);
    assert_eq!(tree.items[children[1]].node, Some(first));
    assert_eq!(tree.items[children[3]].node, Some(second));
    assert_eq!(content(&tree, children[2]).text, "");
    assert_eq!(
        content(&tree, children[2]).instructions,
        vec![
            InlineIns::PushInline(root),
            InlineIns::PushInline(span),
            InlineIns::PopInline,
            InlineIns::PopInline,
        ]
    );

    // The same blocks outside an inline ancestor need no inline fragments.
    let block_root = ui.create(Node::Box, ());
    ui.append(block_root, first);
    ui.append(block_root, second);
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, block_root, &mut tree)
        .unwrap();
    let children: Vec<_> = tree.items.children(key).collect();
    assert_eq!(children.len(), 2);
    assert_eq!(tree.items[children[0]].node, Some(first));
    assert_eq!(tree.items[children[1]].node, Some(second));
}

#[test]
fn deeply_nested_inline_elements_use_an_explicit_stack() {
    let mut ui = Ui::new();
    let mut root = text(&mut ui, "leaf");
    for _ in 0..2000 {
        let parent = ui.create(Node::Box, (DisplayOuter::Inline,));
        ui.append(parent, root);
        root = parent;
    }

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    assert_eq!(tree.items.len(), 1);
    assert_eq!(content(&tree, key).text, "leaf");
    assert_eq!(content(&tree, key).instructions.len(), 4003);
}

#[test]
fn inline_flow_root_is_atomic_and_keeps_block_splitting_inside_its_own_context() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let before = text(&mut ui, "a");
    let atomic = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let after = text(&mut ui, "b");
    ui.append(root, span);
    for child in [before, atomic, after] {
        ui.append(span, child);
    }

    let inner_span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let left = text(&mut ui, "left");
    let block = ui.create(Node::Box, ());
    let right = text(&mut ui, "right");
    ui.append(atomic, inner_span);
    for child in [left, block, right] {
        ui.append(inner_span, child);
    }

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    let runs: Vec<_> = tree.items.children(key).collect();
    assert_eq!(runs.len(), 1);
    let run = runs[0];
    let boxes: Vec<_> = tree.items.children(run).collect();
    assert_eq!(boxes.len(), 1);
    let atomic_key = boxes[0];
    assert_eq!(tree.items[atomic_key].node, Some(atomic));
    assert_eq!(tree.items[atomic_key].ty, LayoutItemTy::Block);
    assert_eq!(content(&tree, run).text, "ab");
    assert_eq!(
        content(&tree, run).instructions,
        vec![
            InlineIns::PushInline(span),
            InlineIns::PushInline(before),
            InlineIns::Text {
                node: before,
                range: 0..1
            },
            InlineIns::PopInline,
            InlineIns::InlineBlock(atomic_key),
            InlineIns::PushInline(after),
            InlineIns::Text {
                node: after,
                range: 1..2
            },
            InlineIns::PopInline,
            InlineIns::PopInline,
        ]
    );

    let inner: Vec<_> = tree.items.children(atomic_key).collect();
    assert_eq!(inner.len(), 3);
    assert_eq!(content(&tree, inner[0]).text, "left");
    assert_eq!(tree.items[inner[1]].node, Some(block));
    assert_eq!(content(&tree, inner[2]).text, "right");
    for &fragment in &[inner[0], inner[2]] {
        assert_eq!(
            content(&tree, fragment).instructions.first(),
            Some(&InlineIns::PushInline(inner_span))
        );
        assert_eq!(
            content(&tree, fragment).instructions.last(),
            Some(&InlineIns::PopInline)
        );
    }

    tree.items.delete_node(key);
    assert!(tree.items.is_empty());
}

#[test]
fn nested_and_empty_inline_flow_roots_remain_atomic() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let nested = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    ui.append(root, nested);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    assert_eq!(tree.items[key].node, Some(root));
    let outer_box = tree.items.first_child(key).unwrap();
    assert_eq!(tree.items[outer_box].node, Some(root));
    assert_eq!(
        content(&tree, key).instructions,
        vec![InlineIns::InlineBlock(outer_box)]
    );
    let inner_run = tree.items.first_child(outer_box).unwrap();
    let inner_box = tree.items.first_child(inner_run).unwrap();
    assert_eq!(tree.items[inner_box].node, Some(nested));
    assert_eq!(
        content(&tree, inner_run).instructions,
        vec![InlineIns::InlineBlock(inner_box)]
    );
    assert_eq!(tree.items.first_child(inner_box), None);
    assert_eq!(tree.items.len(), 4);
}

#[test]
fn content_skips_children_for_both_outer_modes() {
    for outer in [DisplayOuter::Block, DisplayOuter::Inline] {
        let mut ui = Ui::new();
        let root = ui.create(Node::Box, (outer, DisplayInner::Content));
        let ignored = text(&mut ui, "not laid out");
        let ignored_block = ui.create(Node::Box, ());
        ui.append(root, ignored);
        ui.append(root, ignored_block);

        let mut tree = LayoutTree::new();
        let key = LayoutTreeBuilder::new()
            .build_new(&ui, root, &mut tree)
            .unwrap();
        let box_key = if outer == DisplayOuter::Inline {
            let atomic = tree.items.first_child(key).unwrap();
            assert_eq!(content(&tree, key).text, "");
            assert_eq!(
                content(&tree, key).instructions,
                vec![InlineIns::InlineBlock(atomic)]
            );
            assert_eq!(tree.items.len(), 2);
            atomic
        } else {
            assert_eq!(tree.items.len(), 1);
            key
        };
        assert_eq!(tree.items[box_key].ty, LayoutItemTy::Block);
        assert_eq!(tree.items[box_key].node, Some(root));
        assert_eq!(tree.items.first_child(box_key), None);
    }
}

#[test]
fn block_flow_root_still_splits_the_outer_inline_flow() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline,));
    let a = text(&mut ui, "a");
    let block = ui.create(Node::Box, (DisplayOuter::Block, DisplayInner::FlowRoot));
    let own_text = text(&mut ui, "inside");
    let b = text(&mut ui, "b");
    for child in [a, block, b] {
        ui.append(root, child);
    }
    ui.append(block, own_text);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build_new(&ui, root, &mut tree)
        .unwrap();
    let children: Vec<_> = tree.items.children(key).collect();
    assert_eq!(children.len(), 3);
    assert_eq!(content(&tree, children[0]).text, "a");
    assert_eq!(tree.items[children[1]].node, Some(block));
    assert_eq!(tree.items[children[1]].ty, LayoutItemTy::Block);
    assert_eq!(content(&tree, children[2]).text, "b");
    let inner = tree.items.first_child(children[1]).unwrap();
    assert_eq!(content(&tree, inner).text, "inside");
}
