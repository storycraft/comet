use slotmap::new_key_type;
use taffy::Cache;

use crate::{layout::BoxLayout, ui::NodeKey};

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
