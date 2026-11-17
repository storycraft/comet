use std::sync::Arc;

use comet_slotmap_tree::SlotTree;
use comet_tree::node::NodeKey;
use rustc_hash::FxHashMap;

use crate::input::{InputFlags, LayoutInputItem, LayoutInputKey, TextStyle, text::TextStyleBits};

pub struct LayoutInputTree {
    /// Raw tree storage. Use the tree mutation methods and builder to keep
    /// source mappings synchronized.
    pub items: SlotTree<LayoutInputKey, LayoutInputItem>,

    /// 1:1 mapping from [`NodeKey`] to [`LayoutInputKey`]
    map: FxHashMap<NodeKey, LayoutInputKey>,

    /// Number of items whose text style is
    /// [`TextStyleBits::DEPENDS_VIEWPORT`].
    viewport_dependents: usize,
}

impl LayoutInputTree {
    pub fn new() -> Self {
        Self {
            items: SlotTree::new(),
            map: FxHashMap::default(),
            viewport_dependents: 0,
        }
    }

    /// Number of items whose text style is
    /// [`TextStyleBits::DEPENDS_VIEWPORT`].
    pub fn viewport_dependents(&self) -> usize {
        self.viewport_dependents
    }

    /// Find the item generated for a source node.
    /// If a UI is built into independent roots more than once, the most recently
    /// built representation owns the lookup.
    pub fn item_for_node(&self, node: NodeKey) -> Option<LayoutInputKey> {
        self.map.get(&node).copied()
    }

    /// Insert `flags` into the [`LayoutInputItem::flags`] of an item and
    /// into the [`LayoutInputItem::child_flags`] of its ancestors. Stale keys
    /// are ignored.
    ///
    /// Use [`InputFlags::NEED_FULL_LAYOUT`] after props of the source node
    /// resolved into the item changed, such as its text style or display, and
    /// [`InputFlags::NEED_CHILDREN`] after its child list changed. Mark both
    /// parents when moving a source node, and the parent when deleting one.
    pub fn mark(&mut self, key: LayoutInputKey, flags: InputFlags) {
        let Some(item) = self.items.get_mut(key) else {
            return;
        };

        item.flags.insert(flags);
        self.mark_parents(key, flags);
    }

    /// Insert `flags` into the [`LayoutInputItem::child_flags`] of every
    /// ancestor of an item, as [`Self::mark`] does after marking the item.
    pub(crate) fn mark_parents(&mut self, key: LayoutInputKey, flags: InputFlags) {
        let mut ancestor = self.items.parent(key);

        while let Some(key) = ancestor {
            let item = &mut self.items[key];

            // Ancestors of a marked item are marked already.
            if item.child_flags.contains(flags) {
                break;
            }

            item.child_flags.insert(flags);
            ancestor = self.items.parent(key);
        }
    }

    /// Insert `flags` into the item generated for a source node, as
    /// [`Self::mark`]. Unmapped nodes are ignored; mark their parent
    /// [`InputFlags::NEED_CHILDREN`] when inserting them.
    pub fn mark_node(&mut self, node: NodeKey, flags: InputFlags) {
        if let Some(key) = self.item_for_node(node) {
            self.mark(key, flags);
        }
    }

    /// Clear the [`LayoutInputItem::flags`] and
    /// [`LayoutInputItem::child_flags`] of an item and of all of its
    /// descendants. Run it once every step consumed the flags.
    ///
    /// Only subtrees with child flags are visited. Flags of ancestors are
    /// kept, so clear from the root unless other subtrees still need to be
    /// consumed. Stale keys are ignored.
    pub fn clear_flags(&mut self, key: LayoutInputKey) {
        if self.items.get(key).is_none() {
            return;
        }

        let mut pending = vec![key];

        while let Some(key) = pending.pop() {
            let item = &mut self.items[key];
            let child_flags = std::mem::take(&mut item.child_flags);
            item.flags = InputFlags::empty();

            // Flagged descendants always have flagged ancestors.
            if !child_flags.is_empty() {
                pending.extend(self.items.children(key));
            }
        }
    }

    /// Invalidate the items under `root`, including itself, whose text
    /// styles are [`TextStyleBits::DEPENDS_VIEWPORT`], so they resolve again
    /// with a changed viewport. Call it before building after the viewport
    /// changed. Returns at once without viewport dependents. Stale keys are
    /// ignored.
    pub fn invalidate_viewport(&mut self, root: LayoutInputKey) {
        if self.viewport_dependents == 0 || self.items.get(root).is_none() {
            return;
        }

        let mut pending = vec![root];
        while let Some(key) = pending.pop() {
            if depends_viewport(&self.items[key].text_style) {
                self.mark(key, InputFlags::NEED_FULL_LAYOUT);
            }

            pending.extend(self.items.children(key));
        }
    }

    /// Replace the text style of an item, counting viewport dependents.
    pub(crate) fn set_text_style(&mut self, key: LayoutInputKey, text_style: Arc<TextStyle>) {
        let item = &mut self.items[key];

        match (
            depends_viewport(&item.text_style),
            depends_viewport(&text_style),
        ) {
            (false, true) => self.viewport_dependents += 1,
            (true, false) => self.viewport_dependents -= 1,
            _ => {}
        }

        item.text_style = text_style;
    }

    /// Insert a detached item and map its source node to it.
    pub(crate) fn add_item(&mut self, item: LayoutInputItem) -> LayoutInputKey {
        let node = item.node;

        if depends_viewport(&item.text_style) {
            self.viewport_dependents += 1;
        }

        let key = self.items.insert(item);
        self.map.insert(node, key);

        key
    }

    /// Delete a subtree and remove its source mappings.
    pub fn delete_item(&mut self, key: LayoutInputKey) -> Option<LayoutInputItem> {
        self.items.get(key)?;

        for key in self.items.descendants(key) {
            let item = &self.items[key];
            let node = item.node;

            if depends_viewport(&item.text_style) {
                self.viewport_dependents -= 1;
            }

            if self.map.get(&node) == Some(&key) {
                self.map.remove(&node);
            }
        }

        self.items.delete_node(key)
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.map.clear();
        self.viewport_dependents = 0;
    }
}

fn depends_viewport(style: &TextStyle) -> bool {
    style.bits.contains(TextStyleBits::DEPENDS_VIEWPORT)
}

impl Default for LayoutInputTree {
    fn default() -> Self {
        Self::new()
    }
}
