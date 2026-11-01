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

    /// Delete a subtree and reconnect continuation chains that cross its boundary.
    pub fn delete_item(&mut self, key: LayoutItemKey) -> Option<LayoutItem> {
        let item = *self.items.get(key)?;
        self.unlink_continuation(key);
        let mut pending = Vec::new();
        pending.extend(self.items.children(key));

        while let Some(key) = pending.pop() {
            pending.extend(self.items.children(key));
            self.unlink_continuation(key);
        }

        self.items.delete_node(key);

        Some(item)
    }

    pub(crate) fn unlink_continuation(&mut self, key: LayoutItemKey) {
        let Some(item) = self.items.get_mut(key) else {
            return;
        };
        let Continuation { prev, next } = std::mem::take(&mut item.continuation);

        if let Some(item) = prev.and_then(|key| self.items.get_mut(key)) {
            item.continuation.next = next;
        }

        if let Some(item) = next.and_then(|key| self.items.get_mut(key)) {
            item.continuation.prev = prev;
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutItem {
    pub ty: LayoutItemTy,
    pub child_layout: ChildLayoutMode,
    pub continuation: Continuation,
}

/// Links fragments of one source element in layout order, across anonymous runs.
/// An unsplit item has neither link; the first/last fragment has no prev/next.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Continuation {
    pub next: Option<LayoutItemKey>,
    pub prev: Option<LayoutItemKey>,
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
