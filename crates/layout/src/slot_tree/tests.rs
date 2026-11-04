use slotmap::DefaultKey;

use super::SlotTree;

type Tree = SlotTree<DefaultKey, i32>;

#[test]
fn descendants_follow_preorder_and_stop_at_the_subtree_boundary() {
    let mut tree = Tree::new();
    let root = tree.insert(0);
    let first = tree.insert(1);
    let nested = tree.insert(2);
    let last = tree.insert(3);
    let sibling = tree.insert(4);

    tree.append(root, first);
    tree.append(first, nested);
    tree.append(root, last);
    tree.after(root, sibling);

    assert_eq!(
        tree.descendants(root).collect::<Vec<_>>(),
        [root, first, nested, last]
    );
    assert_eq!(tree.descendants(first).collect::<Vec<_>>(), [first, nested]);

    let mut leaf = tree.descendants(last);

    assert_eq!(leaf.next(), Some(last));
    assert_eq!(leaf.next(), None);
    assert_eq!(leaf.next(), None);

    tree.delete_node(first);

    assert_eq!(tree.descendants(first).next(), None);
    assert_eq!(tree.descendants(root).collect::<Vec<_>>(), [root, last]);
}

fn assert_children(tree: &Tree, parent: DefaultKey, expected: &[DefaultKey]) {
    assert_eq!(tree.children(parent).collect::<Vec<_>>(), expected);
    assert_eq!(
        tree.children(parent).rev().collect::<Vec<_>>(),
        expected.iter().rev().copied().collect::<Vec<_>>()
    );
    assert_eq!(tree.first_child(parent), expected.first().copied());
    assert_eq!(tree.last_child(parent), expected.last().copied());

    for (index, &id) in expected.iter().enumerate() {
        assert_eq!(tree.parent(id), Some(parent));
        assert_eq!(
            tree.prev_sibling(id),
            index.checked_sub(1).map(|i| expected[i])
        );
        assert_eq!(tree.next_sibling(id), expected.get(index + 1).copied());
    }
}

#[test]
fn insert_view_mutate_and_clear() {
    let mut tree = Tree::default();
    assert!(tree.is_empty());

    let id = tree.insert(10);
    assert_eq!(tree.len(), 1);
    assert_eq!(tree.get(id), Some(&10));
    *tree.get_mut(id).unwrap() = 20;
    tree[id] += 1;
    assert_eq!(tree[id], 21);
    assert_children(&tree, id, &[]);

    tree.clear();
    assert!(tree.is_empty());
    let replacement = tree.insert(30);
    assert_ne!(id, replacement);
    assert_eq!(tree.get(id), None);
    assert_eq!(tree.get_mut(id), None);
    assert_eq!(tree.parent(id), None);
    assert_eq!(tree.first_child(id), None);
    assert_eq!(tree.last_child(id), None);
    assert_eq!(tree.prev_sibling(id), None);
    assert_eq!(tree.next_sibling(id), None);
}

#[test]
fn insertion_and_reordering_preserve_endpoints() {
    let mut tree = Tree::new();
    let parent = tree.insert(0);
    let a = tree.insert(1);
    let b = tree.insert(2);
    let c = tree.insert(3);
    let d = tree.insert(4);

    tree.append(parent, b);
    tree.prepend(parent, a);
    tree.after(b, d);
    tree.before(d, c);
    assert_children(&tree, parent, &[a, b, c, d]);

    assert_eq!(tree.append(parent, a), Some(parent));
    assert_children(&tree, parent, &[b, c, d, a]);
    assert_eq!(tree.prepend(parent, a), Some(parent));
    assert_children(&tree, parent, &[a, b, c, d]);

    tree.before(b, c);
    assert_children(&tree, parent, &[a, c, b, d]);
    tree.after(b, c);
    assert_children(&tree, parent, &[a, b, c, d]);

    tree.before(b, a);
    tree.after(c, d);
    tree.append(parent, d);
    tree.prepend(parent, a);
    assert_children(&tree, parent, &[a, b, c, d]);
}

#[test]
fn detach_middle_first_last_and_only_child() {
    let mut tree = Tree::new();
    let parent = tree.insert(0);
    let children: Vec<_> = (1..=4).map(|value| tree.insert(value)).collect();
    for &child in &children {
        tree.append(parent, child);
    }

    for (removed, remaining) in [
        (children[1], vec![children[0], children[2], children[3]]),
        (children[0], vec![children[2], children[3]]),
        (children[3], vec![children[2]]),
        (children[2], vec![]),
    ] {
        assert_eq!(tree.remove_parent(removed), Some(parent));
        assert_children(&tree, parent, &remaining);
        assert_eq!(tree.parent(removed), None);
        assert_eq!(tree.prev_sibling(removed), None);
        assert_eq!(tree.next_sibling(removed), None);
        assert_eq!(tree.remove_parent(removed), None);
    }
}

#[test]
fn reparenting_preserves_subtree() {
    let mut tree = Tree::new();
    let p = tree.insert(0);
    let q = tree.insert(1);
    let child = tree.insert(2);
    let grandchild = tree.insert(3);
    tree.append(p, child);
    tree.append(child, grandchild);

    assert_eq!(tree.prepend(q, child), Some(p));
    assert_children(&tree, p, &[]);
    assert_children(&tree, q, &[child]);
    assert_children(&tree, child, &[grandchild]);

    tree.append(q, child);
    tree.prepend(q, child);
    assert_children(&tree, q, &[child]);
}

#[test]
fn parentless_siblings_can_be_detached_moved_and_deleted() {
    let mut tree = Tree::new();
    let a = tree.insert(1);
    let b = tree.insert(2);
    let c = tree.insert(3);
    tree.after(a, c);
    tree.before(c, b);
    assert_eq!(tree.cursor(Some(a)).collect::<Vec<_>>(), [a, b, c]);
    assert_eq!(tree.cursor(Some(a)).rev().collect::<Vec<_>>(), [c, b, a]);

    tree.remove_parent(b);
    assert_eq!(tree.next_sibling(a), Some(c));
    assert_eq!(tree.prev_sibling(c), Some(a));
    tree.before(a, b);
    tree.after(c, a);
    assert_eq!(tree.cursor(Some(b)).collect::<Vec<_>>(), [b, c, a]);

    let parent = tree.insert(0);
    tree.append(parent, c);
    assert_children(&tree, parent, &[c]);
    assert_eq!(tree.next_sibling(b), Some(a));
    assert_eq!(tree.prev_sibling(a), Some(b));
    assert_eq!(tree.delete_node(b), Some(2));
    assert_eq!(tree.prev_sibling(a), None);
    assert_eq!(tree.get(a), Some(&1));
}

#[test]
fn invalid_moves_are_atomic_and_cycles_are_rejected() {
    let mut tree = Tree::new();
    let root = tree.insert(0);
    let child = tree.insert(1);
    let grandchild = tree.insert(2);
    let stale = tree.insert(3);
    tree.delete_node(stale);
    tree.append(root, child);
    tree.append(child, grandchild);

    for operation in [Tree::append, Tree::prepend, Tree::before, Tree::after] {
        assert_eq!(operation(&mut tree, stale, child), None);
        assert_eq!(operation(&mut tree, root, stale), None);
        assert_eq!(operation(&mut tree, child, child), None);
        assert_eq!(operation(&mut tree, grandchild, root), None);
        assert_children(&tree, root, &[child]);
        assert_children(&tree, child, &[grandchild]);
        assert_children(&tree, grandchild, &[]);
    }

    assert_eq!(tree.remove_parent(stale), None);
    assert_eq!(tree.delete_node(stale), None);
    assert_eq!(tree.len(), 3);
}

#[test]
fn deletion_removes_every_descendant_and_retains_siblings() {
    let mut tree = Tree::new();
    let parent = tree.insert(0);
    let left = tree.insert(1);
    let root = tree.insert(2);
    let right = tree.insert(3);
    for child in [left, root, right] {
        tree.append(parent, child);
    }

    let mut deleted = vec![root];
    for value in 4..8 {
        let child = tree.insert(value);
        tree.append(root, child);
        deleted.push(child);
        for value in 8..11 {
            let grandchild = tree.insert(value);
            tree.append(child, grandchild);
            deleted.push(grandchild);
        }
    }

    assert_eq!(tree.delete_node(root), Some(2));
    for id in deleted {
        assert_eq!(tree.get(id), None);
    }
    assert_eq!(tree.len(), 3);
    assert_children(&tree, parent, &[left, right]);

    tree.delete_node(parent);
    assert!(tree.is_empty());
}

#[test]
fn deletion_handles_deep_trees_without_recursion() {
    let mut tree = Tree::new();
    let mut root = tree.insert(0);
    // Build upward so cycle validation stays constant-time per insertion.
    for value in 1..20_000 {
        let parent = tree.insert(value);
        tree.append(parent, root);
        root = parent;
    }

    assert_eq!(tree.delete_node(root), Some(19_999));
    assert!(tree.is_empty());
}

#[test]
fn iterator_range_is_double_ended_and_fused() {
    let mut tree = Tree::new();
    let parent = tree.insert(0);
    let nodes: Vec<_> = (1..=4).map(|value| tree.insert(value)).collect();
    for &id in &nodes {
        tree.append(parent, id);
    }

    let mut iter = tree.children(parent);
    assert_eq!(iter.next(), Some(nodes[0]));
    assert_eq!(iter.next_back(), Some(nodes[3]));
    assert_eq!(iter.next_back(), Some(nodes[2]));
    assert_eq!(iter.next(), Some(nodes[1]));
    assert_eq!(iter.next(), None);
    assert_eq!(iter.next_back(), None);
    assert_eq!(iter.next(), None);

    assert_eq!(
        tree.cursor(Some(nodes[2])).rev().collect::<Vec<_>>(),
        [nodes[3], nodes[2]]
    );
    assert_eq!(
        tree.children(parent).skip(2).collect::<Vec<_>>(),
        nodes[2..]
    );
    assert_eq!(tree.children(parent).nth(3), Some(nodes[3]));
    assert_eq!(tree.cursor(None).next_back(), None);

    let stale = tree.insert(9);
    tree.delete_node(stale);
    let mut empty = tree.cursor(Some(stale));
    assert_eq!(empty.next(), None);
    assert_eq!(empty.next_back(), None);
}
