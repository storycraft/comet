pub mod builder;

use core::ops::Range;

use comet_tree::node::NodeKey;
use slotmap::new_key_type;

use crate::slot_tree::SlotTree;

/// Intermediate layout items. Anonymous inline runs have no associated node.
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

    /// Delete an item and its owned descendants. Remove its inline-box reference
    /// from its parent, when present. A stale key is a no-op.
    pub fn delete_item(&mut self, key: LayoutItemKey) -> Option<LayoutItem> {
        if let Some(parent) = self.items.parent(key)
            && let Some(LayoutItem {
                ty: LayoutItemTy::Inline(inline),
                ..
            }) = self.items.get_mut(parent)
        {
            inline
                .instructions
                .retain(|ins| !matches!(ins, InlineIns::InlineBlock(child) if *child == key));
        }

        self.items.delete_node(key)
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutItem {
    pub ty: LayoutItemTy,
    pub node: Option<NodeKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutItemTy {
    Block,
    Inline(InlineLayoutItem),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InlineLayoutItem {
    /// Concatenated text, preserved verbatim without whitespace normalization.
    pub text: String,
    pub instructions: Vec<InlineIns>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineIns {
    /// A UTF-8 byte range in the enclosing item's text.
    Text(Range<usize>),
    PushInline(NodeKey),
    PopInline,
    /// An atomic inline box with its own layout context. The referenced block
    /// item is a child of this inline item, so subtree deletion owns it too.
    InlineBlock(LayoutItemKey),
}

new_key_type! {
    pub struct LayoutItemKey;
}
