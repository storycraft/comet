pub mod builder;

use comet_tree::node::NodeKey;
use rustc_hash::FxHashMap;
use slotmap::new_key_type;

use crate::slot_tree::SlotTree;

#[derive(Default)]
pub struct LayoutTree {
    /// Raw tree storage. Use the LayoutTree mutation methods and builder to keep
    /// source mappings and continuation links synchronized.
    pub items: SlotTree<LayoutItemKey, LayoutItem>,
    node_items: FxHashMap<NodeKey, LayoutItemKey>,

    // Build ownership cannot be inferred from source mappings: a split inline
    // root is anonymous, and independent builds may share the same source nodes.
    pub(crate) build_roots: FxHashMap<LayoutItemKey, NodeKey>,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_item(&mut self, item: LayoutItem) -> LayoutItemKey {
        let key = self.items.insert(item);

        if let Some(node) = item.ty.node() {
            self.node_items.entry(node).or_insert(key);
        }

        key
    }

    /// Find a source node's first fragment; follow `continuation.next` for the rest.
    /// Anonymous items have no source mapping. If a UI is built into independent
    /// roots more than once, the most recently built representation owns the lookup.
    pub fn item_for_node(&self, node: NodeKey) -> Option<LayoutItemKey> {
        self.node_items.get(&node).copied()
    }

    pub(crate) fn map_first_item(&mut self, node: NodeKey, key: LayoutItemKey) {
        self.node_items.insert(node, key);
    }

    fn unmap_item(&mut self, key: LayoutItemKey) {
        let item = &self.items[key];

        if let Some(node) = item.ty.node()
            && self.node_items.get(&node) == Some(&key)
        {
            if let Some(next) = item.link.next {
                self.node_items.insert(node, next);
            } else {
                self.node_items.remove(&node);
            }
        }
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

        self.build_roots.remove(&key);
        self.unmap_item(key);
        self.unlink_continuation(key);

        let mut pending = Vec::new();
        pending.extend(self.items.children(key));

        while let Some(key) = pending.pop() {
            self.build_roots.remove(&key);
            pending.extend(self.items.children(key));
            self.unmap_item(key);
            self.unlink_continuation(key);
        }

        self.items.delete_node(key);

        Some(item)
    }

    pub(crate) fn unlink_continuation(&mut self, key: LayoutItemKey) {
        let Some(item) = self.items.get_mut(key) else {
            return;
        };
        let LayoutItemLink { prev, next } = std::mem::take(&mut item.link);

        if let Some(item) = prev.and_then(|key| self.items.get_mut(key)) {
            item.link.next = next;
        }

        if let Some(item) = next.and_then(|key| self.items.get_mut(key)) {
            item.link.prev = prev;
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.node_items.clear();
        self.build_roots.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutItem {
    pub ty: LayoutItemTy,
    pub child_layout: ChildLayoutMode,
    pub link: LayoutItemLink,
}

/// Links fragments of one source element in layout order, across anonymous runs.
/// An unsplit item has neither link; the first/last fragment has no prev/next.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LayoutItemLink {
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
    /// Independent direct child boxes arranged by a custom container algorithm.
    Container,
}

new_key_type! {
    pub struct LayoutItemKey;
}
