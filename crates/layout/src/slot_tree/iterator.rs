use core::iter::FusedIterator;
use slotmap::Key;

use super::SlotTree;

/// A double-ended, fused iterator over a fixed sibling range.
/// Unlike a movable cursor, alternating ends never revisits a node.
pub struct Cursor<'a, K: Key, V> {
    tree: &'a SlotTree<K, V>,
    front: Option<K>,
    back: Option<K>,
}

impl<'a, K: Key, V> Cursor<'a, K, V> {
    pub fn new(tree: &'a SlotTree<K, V>, start: Option<K>) -> Self {
        let front = start.filter(|&id| tree.get(id).is_some());
        let mut back = front;

        if let Some(start) = front {
            if let Some(parent) = tree.parent(start) {
                back = tree.last_child(parent);
            } else {
                while let Some(next) = back.and_then(|id| tree.next_sibling(id)) {
                    back = Some(next);
                }
            }
        }

        Self { tree, front, back }
    }
}

impl<K: Key, V> Iterator for Cursor<'_, K, V> {
    type Item = K;

    fn next(&mut self) -> Option<K> {
        let id = self.front?;
        if self.front == self.back {
            self.front = None;
            self.back = None;
        } else {
            self.front = self.tree.next_sibling(id);
        }

        Some(id)
    }
}

impl<K: Key, V> DoubleEndedIterator for Cursor<'_, K, V> {
    fn next_back(&mut self) -> Option<K> {
        let id = self.back?;
        if self.front == self.back {
            self.front = None;
            self.back = None;
        } else {
            self.back = self.tree.prev_sibling(id);
        }

        Some(id)
    }
}

impl<K: Key, V> FusedIterator for Cursor<'_, K, V> {}
