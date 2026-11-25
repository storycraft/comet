//! Inline content of a [`LayoutInputTree`], traversed as a flat list.

use core::iter::FusedIterator;

use crate::input::{LayoutInputKey, LayoutInputKind, tree::LayoutInputTree};

/// An item of the inline content of an inline formatting context, returned
/// by [`InlinePrelayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputItem {
    /// Enter an inline element. Its content follows, up to the matching
    /// [`Self::Pop`]. An empty inline element is a push followed by its pop.
    Push(LayoutInputKey),
    /// Leave the most recently pushed inline element.
    Pop,
    /// A text item.
    Text(LayoutInputKey),
    /// An atomic inline box inside the inline content. Its own content is not
    /// traversed.
    Atomic(LayoutInputKey),
    /// A block level box splitting the inline content. Its own content is not
    /// traversed.
    Block(LayoutInputKey),
}

/// Where the traversal continues.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// Return an item, and enter its content.
    Enter(LayoutInputKey),
    /// Leave an inline element.
    Exit(LayoutInputKey),
}

/// The inline content of an inline formatting context, as a flat list of
/// [`InputItem`]s in tree order.
///
/// The root is the item establishing the inline formatting context. Its
/// children, and those of the inline elements inside it, are its inline
/// content. A text root's content is itself.
///
/// The traversal follows the tree links, so nothing is allocated, and
/// cloning it resumes from the same item.
#[derive(Clone)]
pub struct InlinePrelayout<'a> {
    tree: &'a LayoutInputTree,
    root: LayoutInputKey,
    next: Option<Step>,
}

impl<'a> InlinePrelayout<'a> {
    /// Traverse the inline content of `root`. Stale roots yield nothing.
    pub fn new(tree: &'a LayoutInputTree, root: LayoutInputKey) -> Self {
        let next = match tree.items.get(root) {
            Some(item) if item.kind == LayoutInputKind::Text => Some(Step::Enter(root)),
            Some(_) => tree.items.first_child(root).map(Step::Enter),
            None => None,
        };

        Self { tree, root, next }
    }

    /// Resume the inline content of `root` right after `item`, an atomic
    /// inline or block level box inside it: its content is skipped, and the
    /// pops of the inline elements open around it follow in their turn.
    pub fn after(tree: &'a LayoutInputTree, root: LayoutInputKey, item: LayoutInputKey) -> Self {
        let mut prelayout = Self {
            tree,
            root,
            next: None,
        };

        if tree.items.get(item).is_some() {
            prelayout.next = prelayout.advance(item);
        }

        prelayout
    }

    /// The inline elements of `root` open around `item`, outermost first.
    pub fn open_around(
        tree: &LayoutInputTree,
        root: LayoutInputKey,
        item: LayoutInputKey,
        out: &mut Vec<LayoutInputKey>,
    ) {
        out.clear();
        out.extend(
            std::iter::successors(tree.items.parent(item), |&key| tree.items.parent(key))
                .take_while(|&key| key != root)
                .filter(|&key| tree.items[key].kind == LayoutInputKind::Inline),
        );
        out.reverse();
    }

    pub fn root(&self) -> LayoutInputKey {
        self.root
    }

    pub fn tree(&self) -> &'a LayoutInputTree {
        self.tree
    }

    /// The step after the item `key` and its content.
    fn advance(&self, key: LayoutInputKey) -> Option<Step> {
        if key == self.root {
            return None;
        }

        if let Some(sibling) = self.tree.items.next_sibling(key) {
            return Some(Step::Enter(sibling));
        }

        self.tree
            .items
            .parent(key)
            .filter(|&parent| parent != self.root)
            .map(Step::Exit)
    }
}

impl Iterator for InlinePrelayout<'_> {
    type Item = InputItem;

    fn next(&mut self) -> Option<InputItem> {
        match self.next? {
            Step::Enter(key) => match self.tree.items[key].kind {
                LayoutInputKind::Inline => {
                    self.next = match self.tree.items.first_child(key) {
                        Some(child) => Some(Step::Enter(child)),
                        None => Some(Step::Exit(key)),
                    };

                    Some(InputItem::Push(key))
                }
                LayoutInputKind::Text => {
                    self.next = self.advance(key);
                    Some(InputItem::Text(key))
                }
                LayoutInputKind::AtomicInline => {
                    self.next = self.advance(key);
                    Some(InputItem::Atomic(key))
                }
                LayoutInputKind::Block => {
                    self.next = self.advance(key);
                    Some(InputItem::Block(key))
                }
            },
            Step::Exit(key) => {
                self.next = self.advance(key);
                Some(InputItem::Pop)
            }
        }
    }
}

impl FusedIterator for InlinePrelayout<'_> {}
