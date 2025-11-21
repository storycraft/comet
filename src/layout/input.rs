pub mod cx;
mod inline;

use slotmap::new_key_type;
use taffy::Cache;

use crate::{tree::slot::SlotTree, ui::NodeKey};

#[derive(Debug, Clone)]
pub enum InputNodeTy {
    /// Block node with optional span
    Block(Option<NodeKey>),
    /// Inline node
    Inline(InlineNode),
}

#[derive(Debug, Clone)]
/// A layout input block with constraints
pub struct InputNode {
    pub ty: InputNodeTy,
    pub cache: Cache,
}

impl InputNode {
    pub fn new(ty: InputNodeTy) -> Self {
        Self {
            ty,
            cache: Cache::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InlineNode {
    /// Start to inline content
    pub inline_start: Option<InlineKey>,
    /// Concatenated inline texts
    pub texts: String,
}

impl InlineNode {
    pub fn new(inline_start: Option<InlineKey>) -> Self {
        Self {
            inline_start,
            texts: String::new(),
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
    Node(InputNodeKey),
}

new_key_type! {
    pub struct InputNodeKey;
    pub struct InlineKey;
}

pub struct LayoutInputTree {
    pub nodes: SlotTree<InputNodeKey, InputNode>,
    pub inlines: SlotTree<InlineKey, InlineIns>,
}

impl LayoutInputTree {
    pub fn new() -> Self {
        Self {
            nodes: SlotTree::new(),
            inlines: SlotTree::new(),
        }
    }

    pub fn create_root(&mut self) -> InputNodeKey {
        self.nodes.insert(InputNode::new(InputNodeTy::Block(None)))
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.inlines.clear();
    }

    pub fn delete_node(&mut self, key: InputNodeKey) -> Option<InputNode> {
        let node = self.nodes.delete_node(key)?;
        let InputNodeTy::Inline(ref inline_node) = node.ty else {
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

impl Default for LayoutInputTree {
    fn default() -> Self {
        Self::new()
    }
}
