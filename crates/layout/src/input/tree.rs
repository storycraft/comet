use comet_slotmap_tree::SlotTree;
use comet_tree::node::NodeKey;
use rustc_hash::FxHashMap;

use crate::input::{
    InputFlags, LayoutFlags, LayoutInputItem, LayoutInputKey, LayoutInputKind, text::TextStyleBits,
};

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
    /// Use [`LayoutFlags::NEED_FULL_LAYOUT`] when the item's content, child list or text
    /// style changed. Mark the parent for insertions, and both parents when
    /// moving a source node. Use [`LayoutFlags::NEED_BOX_LAYOUT`] when only its box
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

            if item.flags.contains(LayoutFlags::CHILD_NEED_LAYOUT) {
                break;
            }

            item.flags.insert(LayoutFlags::CHILD_NEED_LAYOUT);
            ancestor = self.items.parent(key);
        }
    }

    /// Mark [`LayoutFlags::NEED_FULL_LAYOUT`] the nearest ancestor of an item
    /// that is not an inline element: the box whose children, or whose
    /// inline content, include the item. Its layout mode depends on the kinds
    /// of those items, so mark it when an item changes kind, or when an
    /// inline element's child list changes.
    pub(crate) fn mark_content_owner(&mut self, key: LayoutInputKey) {
        let mut owner = self.items.parent(key);

        while let Some(key) = owner
            && self.items[key].kind == LayoutInputKind::Inline
        {
            owner = self.items.parent(key);
        }

        if let Some(owner) = owner {
            self.mark(owner, LayoutFlags::NEED_FULL_LAYOUT);
        }
    }

    /// Mark an item whose child list changed [`LayoutFlags::NEED_FULL_LAYOUT`],
    /// and the box owning its content too when it is an inline element.
    pub(crate) fn mark_child_list(&mut self, key: LayoutInputKey) {
        self.mark(key, LayoutFlags::NEED_FULL_LAYOUT);

        if self
            .items
            .get(key)
            .is_some_and(|item| item.kind == LayoutInputKind::Inline)
        {
            self.mark_content_owner(key);
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
            if flags.contains(LayoutFlags::CHILD_NEED_LAYOUT) {
                pending.extend(self.items.children(key));
            }
        }
    }

    /// Insert input `flags` into an item and mark its ancestors
    /// [`InputFlags::CHILD_NEED_INPUT`], so the next
    /// [`LayoutInputTreeBuilder::build`](super::builder::LayoutInputTreeBuilder::build)
    /// from an ancestor resolves the change. Stale keys are ignored.
    ///
    /// Use [`InputFlags::NEED_STYLE`] after props of the source node resolved
    /// into the item changed, such as its text style or display, and
    /// [`InputFlags::NEED_CHILDREN`] after its child list changed. Mark both
    /// parents when moving a source node, and the parent when deleting one.
    pub fn mark_input(&mut self, key: LayoutInputKey, flags: InputFlags) {
        let Some(item) = self.items.get_mut(key) else {
            return;
        };

        item.input_flags.insert(flags);

        let mut ancestor = self.items.parent(key);

        while let Some(key) = ancestor {
            let item = &mut self.items[key];

            // Ancestors of a marked item are marked already.
            if item.input_flags.contains(InputFlags::CHILD_NEED_INPUT) {
                break;
            }

            item.input_flags.insert(InputFlags::CHILD_NEED_INPUT);
            ancestor = self.items.parent(key);
        }
    }

    /// Insert input `flags` into the item generated for a source node, as
    /// [`Self::mark_input`]. Unmapped nodes are ignored; mark their parent
    /// [`InputFlags::NEED_CHILDREN`] when inserting them.
    pub fn mark_input_node(&mut self, node: NodeKey, flags: InputFlags) {
        if let Some(key) = self.item_for_node(node) {
            self.mark_input(key, flags);
        }
    }

    /// Mark the ancestors of a viewport dependent item with
    /// [`TextStyleBits::CHILD_DEPENDS_VIEWPORT`], up to the root. Call it once
    /// the item is attached.
    pub(crate) fn propagate_viewport(&mut self, key: LayoutInputKey) {
        let mut ancestor = self.items.parent(key);

        while let Some(key) = ancestor {
            let item = &mut self.items[key];

            // Ancestors of a marked item are marked already.
            if item.bits.contains(TextStyleBits::CHILD_DEPENDS_VIEWPORT) {
                break;
            }

            item.bits.insert(TextStyleBits::CHILD_DEPENDS_VIEWPORT);
            ancestor = self.items.parent(key);
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
