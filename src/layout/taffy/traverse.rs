use taffy::{TraversePartialTree, TraverseTree};

use crate::{
    layout::{
        taffy::{TaffyLayoutImpl, from_taffy_key, to_taffy_key},
        tree::{LayoutBox, LayoutBoxKey},
    },
    tree::cursor::Cursor,
};

impl TraversePartialTree for TaffyLayoutImpl<'_> {
    type ChildIter<'a>
        = ChildIter<'a>
    where
        Self: 'a;

    fn child_ids(&self, parent_node_id: taffy::NodeId) -> Self::ChildIter<'_> {
        let first_id = self
            .layout_tree
            .boxes
            .first_child(from_taffy_key(parent_node_id));

        ChildIter {
            iter: self.layout_tree.boxes.cursor(first_id),
        }
    }

    fn child_count(&self, parent_node_id: taffy::NodeId) -> usize {
        self.layout_tree
            .boxes
            .cursor(
                self.layout_tree
                    .boxes
                    .first_child(from_taffy_key(parent_node_id)),
            )
            .count()
    }

    fn get_child_id(&self, parent_node_id: taffy::NodeId, child_index: usize) -> taffy::NodeId {
        self.child_ids(parent_node_id).nth(child_index).unwrap()
    }
}

impl TraverseTree for TaffyLayoutImpl<'_> {}

pub(crate) struct ChildIter<'a> {
    iter: Cursor<'a, LayoutBoxKey, LayoutBox>,
}

impl Iterator for ChildIter<'_> {
    type Item = taffy::NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        Some(to_taffy_key(self.iter.next()?))
    }
}
