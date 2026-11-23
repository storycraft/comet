//! Text selection.
//!
//! Positions are kept in the text of an inline formatting context, by the
//! node of its box and a byte offset, so they stay valid when the fragments
//! are built again. A selection runs from its anchor, where it started, to
//! its extent, where it ends, in either order.

use comet_tree::node::NodeKey;
use parley::Cursor;

/// Which side of a position the caret belongs to, where it is ambiguous,
/// such as at a soft line break.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Affinity {
    /// The character following the position.
    #[default]
    Downstream,
    /// The character preceding the position.
    Upstream,
}

impl From<parley::Affinity> for Affinity {
    fn from(affinity: parley::Affinity) -> Self {
        match affinity {
            parley::Affinity::Downstream => Self::Downstream,
            parley::Affinity::Upstream => Self::Upstream,
        }
    }
}

impl From<Affinity> for parley::Affinity {
    fn from(affinity: Affinity) -> Self {
        match affinity {
            Affinity::Downstream => Self::Downstream,
            Affinity::Upstream => Self::Upstream,
        }
    }
}

/// A position in the text of an inline formatting context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextPosition {
    /// The box of the inline formatting context, owning its inline layout.
    pub node: NodeKey,
    /// Byte offset into the text of the inline layout: the text after white
    /// space processing, where each inline box is one U+FFFC character.
    pub index: usize,
    pub affinity: Affinity,
}

impl TextPosition {
    pub const fn new(node: NodeKey, index: usize, affinity: Affinity) -> Self {
        Self {
            node,
            index,
            affinity,
        }
    }

    pub(crate) fn from_cursor(node: NodeKey, cursor: Cursor) -> Self {
        Self::new(node, cursor.index(), cursor.affinity().into())
    }
}

/// A selected range of text, from where it started to where it ends.
///
/// Both ends may be in different inline formatting contexts: the selection
/// then covers the text between them in tree order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Selection {
    /// Where the selection started, fixed while it is extended.
    pub anchor: TextPosition,
    /// Where the selection ends, moved while it is extended.
    pub extent: TextPosition,
}

impl Selection {
    pub const fn new(anchor: TextPosition, extent: TextPosition) -> Self {
        Self { anchor, extent }
    }

    /// An empty selection at `position`, a caret.
    pub const fn collapsed(position: TextPosition) -> Self {
        Self::new(position, position)
    }

    /// Whether it selects no text.
    pub fn is_collapsed(&self) -> bool {
        self.anchor.node == self.extent.node && self.anchor.index == self.extent.index
    }

    pub(crate) fn from_parley(node: NodeKey, selection: parley::Selection) -> Self {
        Self::new(
            TextPosition::from_cursor(node, selection.anchor()),
            TextPosition::from_cursor(node, selection.focus()),
        )
    }
}
