use core::slice;

use crate::tree::{LayoutNodeKey, LayoutTree};

/// Fast nested layout children buffer in flat structure
pub struct ChildrenStack {
    children: Vec<LayoutNodeKey>,
    stack: Vec<usize>,
}

impl ChildrenStack {
    pub fn new() -> Self {
        Self {
            children: vec![],
            stack: vec![],
        }
    }

    pub fn get_child(&self, index: usize) -> Option<LayoutNodeKey> {
        let offset = self.stack.last().copied()?;
        self.children.get(offset + index).copied()
    }

    pub fn iter<'a>(&'a self) -> slice::Iter<'a, LayoutNodeKey> {
        let offset = self.stack.last().copied().unwrap_or_default();
        self.children[offset..].iter()
    }

    pub fn len(&self) -> usize {
        self.children.len() - self.stack.last().copied().unwrap_or_default()
    }

    #[inline]
    pub fn push(&mut self, tree: &LayoutTree, parent: LayoutNodeKey) {
        self.stack.push(self.children.len());

        for child in tree.nodes.cursor(tree.nodes.first_child(parent)) {
            self.children.push(child);
        }
    }

    pub fn pop(&mut self) {
        let Some(start) = self.stack.pop() else {
            return;
        };

        _ = self.children.drain(start..);
    }
}
