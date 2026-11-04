use comet_tree::node::NodeKey;

use crate::tree::{LayoutItemKey, LayoutItemTy, LayoutTree};

pub(super) enum Op {
    Push(NodeKey),
    Pop,
    Text(NodeKey),
    Box(LayoutItemKey),
    Empty(LayoutItemKey),
}

enum Phase {
    Enter,
    Content,
    Exit,
    Advance,
}

/// A repeatable stream of inline input events. Tree links carry traversal state,
/// so recursive atomic layout needs neither an operation buffer nor a stack.
pub(super) struct InlineEvents<'a> {
    tree: &'a LayoutTree,
    root: LayoutItemKey,
    current: Option<LayoutItemKey>,
    phase: Phase,
}

impl<'a> InlineEvents<'a> {
    pub fn new(tree: &'a LayoutTree, root: LayoutItemKey) -> Self {
        let phase = if matches!(tree.items[root].ty, LayoutItemTy::Text(_)) {
            Phase::Enter
        } else {
            Phase::Content
        };

        Self {
            tree,
            root,
            current: Some(root),
            phase,
        }
    }
}

impl Iterator for InlineEvents<'_> {
    type Item = Op;

    fn next(&mut self) -> Option<Op> {
        loop {
            let key = self.current?;

            match self.phase {
                Phase::Enter => {
                    self.phase = Phase::Advance;

                    match self.tree.items[key].ty {
                        LayoutItemTy::Text(node) => return Some(Op::Text(node)),
                        LayoutItemTy::AtomicInline(_) => return Some(Op::Box(key)),
                        ty => {
                            self.phase = Phase::Content;

                            if let Some(node) = ty.node() {
                                return Some(Op::Push(node));
                            }
                        }
                    }
                }
                Phase::Content => {
                    if let Some(child) = self.tree.items.first_child(key) {
                        self.current = Some(child);
                        self.phase = Phase::Enter;
                    } else {
                        self.phase = Phase::Exit;

                        if key != self.root {
                            return Some(Op::Empty(key));
                        }
                    }
                }
                Phase::Exit => {
                    self.phase = Phase::Advance;

                    if key != self.root && self.tree.items[key].ty.node().is_some() {
                        return Some(Op::Pop);
                    }
                }
                Phase::Advance => {
                    if key == self.root {
                        self.current = None;
                    } else if let Some(sibling) = self.tree.items.next_sibling(key) {
                        self.current = Some(sibling);
                        self.phase = Phase::Enter;
                    } else {
                        self.current = self.tree.items.parent(key);
                        self.phase = Phase::Exit;
                    }
                }
            }
        }
    }
}
