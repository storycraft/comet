use slotmap::Key;

use crate::slot_tree::SlotTree;

pub struct Cursor<'a, K: Key, V> {
    tree: &'a SlotTree<K, V>,
    current: Option<K>,
}

impl<'a, K: Key, V> Cursor<'a, K, V> {
    #[inline]
    pub fn new(tree: &'a SlotTree<K, V>, start: Option<K>) -> Self {
        Self {
            tree,
            current: start,
        }
    }
}

impl<'a, K: Key, V> Iterator for Cursor<'a, K, V> {
    type Item = K;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current?;
        self.current = self.tree.next_sibling(current);
        Some(current)
    }
}

impl<'a, K: Key, V> DoubleEndedIterator for Cursor<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let current = self.current?;
        self.current = self.tree.prev_sibling(current);
        Some(current)
    }
}
