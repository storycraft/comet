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

    pub fn clear(&mut self, tree: &LayoutInputTree, root: LayoutInputKey) {
        let mut pending = vec![root];
        while let Some(key) = pending.pop() {
            let Some(entry) = self.entry_mut(key) else {
                continue;
            };

            let old_entry = std::mem::take(entry);
            if !old_entry.child_bits.is_empty() {
                pending.extend(tree.items.children(key));
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
