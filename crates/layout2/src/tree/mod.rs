pub mod builder;

use comet_tree::node::NodeKey;
use slotmap::new_key_type;

use crate::slot_tree::SlotTree;

#[derive(Default)]
pub struct LayoutTree {
    pub items: SlotTree<LayoutItemKey, LayoutItem>,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_item(&mut self, item: LayoutItem) -> LayoutItemKey {
        self.items.insert(item)
    }

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

    pub fn delete_item(&mut self, key: LayoutItemKey) -> Option<LayoutItem> {
        self.items.delete_node(key)
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutItem {
    pub ty: LayoutItemTy,
    pub child_layout: ChildLayoutMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutItemTy {
    Block(NodeKey),
    Inline(NodeKey),
    AnonymousBlock,
    AtomicInline(NodeKey),
    Text(NodeKey),
}

impl LayoutItemTy {
    pub fn node(self) -> Option<NodeKey> {
        match self {
            Self::Block(node)
            | Self::Inline(node)
            | Self::AtomicInline(node)
            | Self::Text(node) => Some(node),

            Self::AnonymousBlock => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildLayoutMode {
    None,
    Block,
    Inline,
}

new_key_type! {
    pub struct LayoutItemKey;
}
