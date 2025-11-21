use core::slice;

use taffy::TraversePartialTree;

use crate::layout::{
    taffy::{TaffyLayout, to_taffy_key},
    tree::node::LayoutNodeKey,
};

impl TraversePartialTree for TaffyLayout<'_> {
    type ChildIter<'a>
        = ChildIter<'a>
    where
        Self: 'a;

    fn child_ids(&self, _: taffy::NodeId) -> Self::ChildIter<'_> {
        ChildIter {
            iter: self.cx.children.iter(),
        }
    }

    fn child_count(&self, _: taffy::NodeId) -> usize {
        self.cx.children.len()
    }

    fn get_child_id(&self, _: taffy::NodeId, child_index: usize) -> taffy::NodeId {
        to_taffy_key(self.cx.children.get_child(child_index).unwrap())
    }
}

pub struct ChildIter<'a> {
    iter: slice::Iter<'a, LayoutNodeKey>,
}

impl Iterator for ChildIter<'_> {
    type Item = taffy::NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        Some(to_taffy_key(*self.iter.next()?))
    }
}
