use comet_tree::node::NodeKey;
use slotmap::SecondaryMap;

use crate::input::{InputFlags, LayoutInputKey, tree::LayoutInputTree};

#[derive(Debug, Clone, Default)]
pub struct DirtyMap {
    map: SecondaryMap<LayoutInputKey, DirtyEntry>,
}

impl DirtyMap {
    pub fn new() -> Self {
        Self {
            map: SecondaryMap::new(),
        }
    }

    pub fn entry(&self, key: LayoutInputKey) -> DirtyEntry {
        self.map.get(key).copied().unwrap_or_default()
    }

    pub fn mark_node(&mut self, tree: &LayoutInputTree, node: NodeKey, flags: InputFlags) {
        if let Some(key) = tree.item_for_node(node) {
            self.mark(tree, key, flags);
        }
    }

    pub fn mark(&mut self, tree: &LayoutInputTree, key: LayoutInputKey, flags: InputFlags) {
        if self.mark_flags(key, flags) {
            self.mark_parents(tree, key, flags);
        }
    }

    /// Clear the flags of `root` and its descendants. Only subtrees with
    /// child bits are visited, in preorder along the parent and sibling
    /// links, so nothing is allocated. Stale keys are ignored.
    pub fn clear(&mut self, tree: &LayoutInputTree, root: LayoutInputKey) {
        if tree.items.get(root).is_none() {
            return;
        }

        let mut current = root;

        loop {
            let child_bits = self
                .map
                .get_mut(current)
                .map(|entry| std::mem::take(entry).child_bits)
                .unwrap_or_default();

            // Flagged descendants always have flagged ancestors.
            if !child_bits.is_empty()
                && let Some(child) = tree.items.first_child(current)
            {
                current = child;
                continue;
            }

            // Next sibling of the nearest ancestor that has one, up to `root`.
            loop {
                if current == root {
                    return;
                }

                if let Some(sibling) = tree.items.next_sibling(current) {
                    current = sibling;
                    break;
                }

                match tree.items.parent(current) {
                    Some(parent) => current = parent,
                    None => return,
                }
            }
        }
    }

    fn mark_parents(&mut self, tree: &LayoutInputTree, key: LayoutInputKey, flags: InputFlags) {
        let mut ancestor = tree.items.parent(key);
        while let Some(key) = ancestor {
            if !self.mark_child_flags(key, flags) {
                break;
            }

            ancestor = tree.items.parent(key);
        }
    }

    fn entry_mut(&mut self, key: LayoutInputKey) -> Option<&mut DirtyEntry> {
        Some(self.map.entry(key)?.or_default())
    }

    fn mark_flags(&mut self, key: LayoutInputKey, flags: InputFlags) -> bool {
        let Some(entry) = self.entry_mut(key) else {
            return false;
        };

        let should_mark = !entry.flags.contains(flags);
        if should_mark {
            entry.flags.insert(flags);
        }
        should_mark
    }

    fn mark_child_flags(&mut self, key: LayoutInputKey, flags: InputFlags) -> bool {
        let Some(entry) = self.entry_mut(key) else {
            return false;
        };

        let should_mark = !entry.child_bits.contains(flags);
        if should_mark {
            entry.child_bits.insert(flags);
        }
        should_mark
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DirtyEntry {
    pub child_bits: InputFlags,
    pub flags: InputFlags,
}
