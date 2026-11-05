use comet_layout::tree::{LayoutItem, LayoutItemKey, LayoutTree, builder::LayoutTreeBuilder};
use comet_tree::{node::NodeKey, ui::Ui};

pub fn keys(tree: &LayoutTree, root: LayoutItemKey) -> Vec<LayoutItemKey> {
    tree.items.descendants(root).collect()
}

pub fn snapshot(tree: &LayoutTree, root: LayoutItemKey) -> Vec<(usize, LayoutItem)> {
    let mut pending = vec![(root, 0)];
    let mut result = Vec::new();

    while let Some((key, depth)) = pending.pop() {
        let mut item = tree.items[key];
        item.link = Default::default();
        result.push((depth, item));
        pending.extend(tree.items.children(key).rev().map(|key| (key, depth + 1)));
    }

    result
}

pub fn assert_fresh(ui: &Ui, source: NodeKey, tree: &LayoutTree, root: LayoutItemKey) {
    let mut fresh = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build(ui, source, &mut fresh, None)
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
                let continuation = tree.items[key].link;

                (
                    continuation.prev.map(|key| positions[&key]),
                    continuation.next.map(|key| positions[&key]),
                )
            })
            .collect::<Vec<_>>()
    };

    assert_eq!(links(tree, root), links(&fresh, key));
}
