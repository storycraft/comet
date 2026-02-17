use comet_div::ui::NodeKey;
use slotmap::new_key_type;

use crate::{
    layout::{BoxLayout, tree::LayoutNodeKey},
    tree::slot::SlotTree,
};

pub struct InlineTree {
    pub nodes: SlotTree<InlineNodeKey, InlineNode>,
}

impl InlineTree {
    pub fn new() -> Self {
        Self {
            nodes: SlotTree::new(),
        }
    }

    /// Delete lines from line_start
    pub fn delete_lines(&mut self, line_start: InlineNodeKey) {
        let mut next_sibling = Some(line_start);
        while let Some(id) = next_sibling {
            next_sibling = self.nodes.next_sibling(id);
            self.nodes.delete_node(id);
        }
    }
}

impl Default for InlineTree {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct InlineNode {
    pub ty: InlineNodeTy,
    pub part: InlineNodePart,
    pub layout: BoxLayout,
}

impl InlineNode {
    pub fn new(ty: InlineNodeTy) -> Self {
        Self::new_parted(ty, InlineNodePart::Full)
    }

    pub fn new_parted(ty: InlineNodeTy, part: InlineNodePart) -> Self {
        Self {
            ty,
            part,
            layout: BoxLayout::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum InlineNodeTy {
    LayoutNode(LayoutNodeKey),
    Box(Option<NodeKey>),
    Text(InlineRun),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineNodePart {
    Full,
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, Copy)]
pub struct InlineRun {
    pub run_index: usize,
    pub cluster_start: usize,
    pub cluster_end: usize,
}

new_key_type! {
    pub struct InlineNodeKey;
}
