use crate::tree2::{EntityId, ArchetypalTree};

pub struct Cursor<'a> {
    tree: &'a ArchetypalTree,
    current: Option<EntityId>,
}

impl<'a> Cursor<'a> {
    #[inline]
    pub fn new(tree: &'a ArchetypalTree, start: Option<EntityId>) -> Self {
        Self {
            tree,
            current: start,
        }
    }
}

impl<'a> Iterator for Cursor<'a> {
    type Item = EntityId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current?;
        self.current = self.tree.next_sibling(current);
        Some(current)
    }
}

impl<'a> DoubleEndedIterator for Cursor<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let current = self.current?;
        self.current = self.tree.prev_sibling(current);
        Some(current)
    }
}
