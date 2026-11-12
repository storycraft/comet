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
        let props = start.and_then(|key| ui.props(key));
        let front = props.and(start);
        let back = props.and_then(|props| match props.parent() {
            Some(parent) => ui.last_child(parent),
            None => start,
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
