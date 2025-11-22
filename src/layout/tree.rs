use slotmap::{SlotMap, new_key_type};
use taffy::Cache;

use crate::{layout::BoxLayout, tree::slot::SlotTree, ui::NodeKey};

pub struct LayoutTree {
    pub nodes: SlotTree<LayoutNodeKey, LayoutNode>,
    pub inline_nodes: SlotMap<InlineLayoutNodeKey, InlineLayoutNode>,
    pub inlines: SlotTree<InlineKey, InlineIns>,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self {
            nodes: SlotTree::new(),
            inline_nodes: SlotMap::with_key(),
            inlines: SlotTree::new(),
        }
    }

    pub fn create_root(&mut self) -> LayoutNodeKey {
        self.nodes
            .insert(LayoutNode::new(LayoutNodeTy::Block(None)))
    }

    pub fn invalidate(&mut self, key: LayoutNodeKey) {
        let Some(node) = self.nodes.get_mut(key) else {
            return;
        };
        if node.cache.is_empty() {
            return;
        }

        node.cache.clear();
        if let Some(parent) = self.nodes.parent(key) {
            self.invalidate(parent);
        }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.inlines.clear();
    }

    pub fn delete_node(&mut self, key: LayoutNodeKey) -> Option<LayoutNode> {
        let node = self.nodes.delete_node(key)?;
        let LayoutNodeTy::Inline(inline_node_key) = node.ty else {
            return Some(node);
        };

        let Some(inline_node) = self.inline_nodes.remove(inline_node_key) else {
            return Some(node);
        };

        let mut next_id = inline_node.inline_start;
        while let Some(id) = next_id {
            next_id = self.inlines.next_sibling(id);
            if let Some(deleted) = self.inlines.delete_node(id)
                && let InlineIns::Node(input_node) = deleted
            {
                self.delete_node(input_node);
            }
        }

        Some(node)
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
/// A layout input block with constraints
pub struct LayoutNode {
    /// Type of the [`LayoutNode`]
    pub ty: LayoutNodeTy,
    /// Cached constraints
    pub cache: Cache,
    /// Resolved layout
    pub layout: BoxLayout,
}

#[derive(Debug, Clone, Copy)]
pub enum LayoutNodeTy {
    /// Block node with optional span
    Block(Option<NodeKey>),
    /// A inline node
    Inline(InlineLayoutNodeKey),
}

impl LayoutNode {
    pub fn new(ty: LayoutNodeTy) -> Self {
        Self {
            ty,
            cache: Cache::new(),
            layout: BoxLayout::new(),
        }
    }
}

#[derive(Clone)]
pub struct InlineLayoutNode {
    /// Start to inline content
    pub inline_start: Option<InlineKey>,
    /// Concatenated inline texts
    pub texts: String,
    /// Inline text layout
    pub layout: parley::Layout<()>,
}

impl InlineLayoutNode {
    pub fn new(inline_start: Option<InlineKey>) -> Self {
        Self {
            inline_start,
            texts: String::new(),
            layout: parley::Layout::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InlineIns {
    /// A text with length
    Text(usize),
    /// Push new inline box
    PushInlineBox(NodeKey),
    /// Pop inline box
    PopInlineBox,
    /// A new Node
    Node(LayoutNodeKey),
}

new_key_type! {
    pub struct LayoutNodeKey;
    pub struct InlineLayoutNodeKey;
    pub struct InlineKey;
}
