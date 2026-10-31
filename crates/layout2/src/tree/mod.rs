pub mod builder;

use comet_tree::node::NodeKey;
use slotmap::new_key_type;

use crate::slot_tree::SlotTree;

/// An object tree of block, inline, and text items. Anonymous items have no
/// source node; text is read from the associated UI node rather than copied here.
#[derive(Default)]
pub struct LayoutTree {
    pub items: SlotTree<LayoutItemKey, LayoutItem>,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a detached item. Keys belong to the tree that created them.
    pub fn add_item(&mut self, item: LayoutItem) -> LayoutItemKey {
        self.items.insert(item)
    }

    /// Add a child, or return `None` without allocating if the parent is stale.
    pub fn append_item(
        &mut self,
        parent: LayoutItemKey,
        item: LayoutItem,
    ) -> Option<LayoutItemKey> {
        self.items.get(parent)?;

        let key = self.add_item(item);
        self.items.append(parent, key);

        Some(key)
    }

    /// Delete an item and its owned descendants. A stale key is a no-op.
    pub fn delete_item(&mut self, key: LayoutItemKey) -> Option<LayoutItem> {
        self.items.delete_node(key)
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutItem {
    /// How this item's children are laid out, or `Text` for a text leaf.
    pub ty: LayoutItemTy,
    pub node: Option<NodeKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutItemTy {
    /// Children participate in block flow.
    Block,
    /// Children participate in inline flow.
    Inline,

    /// Anonymous inline-flow text, without an element box or its own styles.
    /// The source node identifies the text content; the builder reads no properties.
    Text,
}

new_key_type! {
    pub struct LayoutItemKey;
}
