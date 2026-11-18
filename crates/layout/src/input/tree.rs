use std::sync::Arc;

use comet_slotmap_tree::SlotTree;
use comet_tree::node::NodeKey;
use rustc_hash::FxHashMap;

use crate::dirty_map::{DirtyFlags, DirtyMap};
use crate::input::{LayoutInputItem, LayoutInputKey, TextStyle, text::TextStyleBits};

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

    /// Mark the items under `root`, including itself, whose text styles are
    /// [`TextStyleBits::DEPENDS_VIEWPORT`] [`DirtyFlags::NEED_FULL_LAYOUT`] in
    /// `dirty`, so they resolve again with a changed viewport. Call it before
    /// building after the viewport changed. Returns at once without viewport
    /// dependents. Stale keys are ignored.
    pub fn invalidate_viewport(&self, dirty: &mut DirtyMap, root: LayoutInputKey) {
        if self.viewport_dependents == 0 || self.items.get(root).is_none() {
            return;
        }

        for key in self.items.descendants(root) {
            if depends_viewport(&self.items[key].text_style) {
                dirty.mark(self, key, DirtyFlags::NEED_FULL_LAYOUT);
            }
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
