use crate::ui::NodeKey;

pub struct Cursor<'a>(pub(super) crate::tree2::cursor::Cursor<'a>);

impl<'a> Iterator for Cursor<'a> {
    type Item = NodeKey;

    fn next(&mut self) -> Option<Self::Item> {
        Some(NodeKey(self.0.next()?))
    }
}

impl<'a> DoubleEndedIterator for Cursor<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        Some(NodeKey(self.0.next_back()?))
    }
}
