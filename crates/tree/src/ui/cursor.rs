use core::iter::FusedIterator;

use crate::node::NodeKey;

use super::Ui;

pub struct Cursor<'a> {
    ui: &'a Ui,

    front: Option<NodeKey>,
    back: Option<NodeKey>,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(ui: &'a Ui, start: Option<NodeKey>) -> Self {
        let front = start.filter(|key| ui.ui_node(key.0).is_some());
        let back = front.and_then(|key| match ui.parent(key) {
            Some(parent) => ui.last_child(parent),
            None => Some(key),
        });

        Self { ui, front, back }
    }
}

impl Iterator for Cursor<'_> {
    type Item = NodeKey;

    fn next(&mut self) -> Option<NodeKey> {
        let key = self.front?;

        if self.front == self.back {
            self.front = None;
            self.back = None;
        } else {
            self.front = self.ui.next_sibling(key);
        }

        Some(key)
    }
}

impl DoubleEndedIterator for Cursor<'_> {
    fn next_back(&mut self) -> Option<NodeKey> {
        let key = self.back?;

        if self.front == self.back {
            self.front = None;
            self.back = None;
        } else {
            self.back = self.ui.prev_sibling(key);
        }

        Some(key)
    }
}

impl FusedIterator for Cursor<'_> {}
