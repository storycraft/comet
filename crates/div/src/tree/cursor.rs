use hecs::Entity;

use crate::tree::ArchetypalTree;

pub struct Cursor<'a> {
    tree: &'a ArchetypalTree,
    current: Option<Entity>,
}

impl<'a> Cursor<'a> {
    #[inline]
    pub fn new(tree: &'a ArchetypalTree, start: Option<Entity>) -> Self {
        Self {
            tree,
            current: start,
        }
    }
}

impl<'a> Iterator for Cursor<'a> {
    type Item = Entity;

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
