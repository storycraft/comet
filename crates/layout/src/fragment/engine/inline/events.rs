use crate::input::{LayoutInputKey, LayoutInputKind, tree::LayoutInputTree};

pub(super) enum Op {
    /// Enter an inline element.
    Push(LayoutInputKey),
    Pop,
    Text(LayoutInputKey),
    /// Atomic inline box.
    Box(LayoutInputKey),
    /// Block level box interrupting the inline content.
    Block(LayoutInputKey),
    /// Inline element without children.
    Empty(LayoutInputKey),
}

enum Phase {
    Enter,
    Content,
    Exit,
    Advance,
}

/// A repeatable stream of the inline content of an inline formatting context
/// root. Tree links carry traversal state, so no stack is allocated.
pub(super) struct InlineEvents<'a> {
    tree: &'a LayoutInputTree,
    root: LayoutInputKey,
    current: Option<LayoutInputKey>,
    phase: Phase,
}

impl<'a> InlineEvents<'a> {
    pub fn new(tree: &'a LayoutInputTree, root: LayoutInputKey) -> Self {
        let phase = if tree.items[root].kind == LayoutInputKind::Text {
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

    /// Resume the stream of `root` right after a block box in its content.
    pub fn after(tree: &'a LayoutInputTree, root: LayoutInputKey, block: LayoutInputKey) -> Self {
        Self {
            tree,
            root,
            current: Some(block),
            phase: Phase::Advance,
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

                    match self.tree.items[key].kind {
                        LayoutInputKind::Text => return Some(Op::Text(key)),
                        LayoutInputKind::AtomicInline => return Some(Op::Box(key)),
                        LayoutInputKind::Block => return Some(Op::Block(key)),
                        LayoutInputKind::Inline => {
                            self.phase = Phase::Content;
                            return Some(Op::Push(key));
                        }
                    }
                }
                Phase::Content => {
                    if let Some(child) = self.tree.items.first_child(key)
                        && (key == self.root
                            || self.tree.items[key].kind == LayoutInputKind::Inline)
                    {
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

                    if key != self.root {
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
