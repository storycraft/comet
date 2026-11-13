use comet_slotmap_tree::SlotTree;
use comet_tree::node::NodeKey;
use rustc_hash::FxHashMap;

use crate::input::{LayoutFlags, LayoutInputItem, LayoutInputKey};

pub struct LayoutInputTree {
    /// Raw tree storage. Use the tree mutation methods and builder to keep
    /// source mappings synchronized.
    pub items: SlotTree<LayoutInputKey, LayoutInputItem>,

    /// 1:1 mapping from [`NodeKey`] to [`LayoutInputKey`]
    map: FxHashMap<NodeKey, LayoutInputKey>,
}

impl LayoutInputTree {
    pub fn new() -> Self {
        Self {
            items: SlotTree::new(),
            map: FxHashMap::default(),
        }
    }

    /// Find the item generated for a source node.
    /// If a UI is built into independent roots more than once, the most recently
    /// built representation owns the lookup.
    pub fn item_for_node(&self, node: NodeKey) -> Option<LayoutInputKey> {
        self.map.get(&node).copied()
    }

    /// Insert `flags` into an item and propagate descendant dirtiness to its
    /// ancestors. Stale keys are ignored.
    ///
    /// Use [`LayoutFlags::FULL_DIRTY`] when the item's content, child list or text
    /// style changed. Mark the parent for insertions, and both parents when
    /// moving a source node. Use [`LayoutFlags::BOX_DIRTY`] when only its box
    /// style changed, such as its size, padding, margin or insets, to keep its
    /// shaped text and child fragments.
    pub fn mark(&mut self, key: LayoutInputKey, flags: LayoutFlags) {
        let Some(item) = self.items.get_mut(key) else {
            return;
        };

        item.flags.insert(flags);

        let mut ancestor = self.items.parent(key);

        while let Some(key) = ancestor {
            let item = &mut self.items[key];

            if item.flags.contains(LayoutFlags::CHILDREN_DIRTY) {
                break;
            }

            item.flags.insert(LayoutFlags::CHILDREN_DIRTY);
            ancestor = self.items.parent(key);
        }
    }

    /// Insert `flags` into the item generated for a source node, as
    /// [`Self::mark`]. Unmapped nodes are ignored; mark their parent when
    /// inserting new content.
    pub fn mark_node(&mut self, node: NodeKey, flags: LayoutFlags) {
        if let Some(key) = self.item_for_node(node) {
            self.mark(key, flags);
        }
    }

    /// Clear the flags of an item and all of its descendants. Run it after
    /// consuming the flags, such as after building fragments.
    ///
    /// Only flagged subtrees are visited. Flags of ancestors are kept, so clear
    /// from the root unless other subtrees still need to be consumed.
    /// Stale keys are ignored.
    pub fn clear_flags(&mut self, key: LayoutInputKey) {
        if self.items.get(key).is_none() {
            return;
        }

        let mut pending = vec![key];

        while let Some(key) = pending.pop() {
            let flags = std::mem::take(&mut self.items[key].flags);

            // Flagged descendants always have flagged ancestors.
            if flags.contains(LayoutFlags::CHILDREN_DIRTY) {
                pending.extend(self.items.children(key));
            }
        }
    }

    /// Insert a detached item and map its source node to it.
    pub(crate) fn add_item(&mut self, item: LayoutInputItem) -> LayoutInputKey {
        let node = item.node;
        let key = self.items.insert(item);
        self.map.insert(node, key);

        key
    }

    /// Delete a subtree and remove its source mappings.
    pub fn delete_item(&mut self, key: LayoutInputKey) -> Option<LayoutInputItem> {
        self.items.get(key)?;

        for key in self.items.descendants(key) {
            let node = self.items[key].node;

            if self.map.get(&node) == Some(&key) {
                self.map.remove(&node);
            }
        }

        self.items.delete_node(key)
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.map.clear();
    }
}

impl Default for LayoutInputTree {
    fn default() -> Self {
        Self::new()
    }
}
