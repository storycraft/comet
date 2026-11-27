//! Text selection.
//!
//! Positions are kept in the text of text nodes, by the node and a byte
//! offset into its text as written, so they stay valid when the fragments
//! are built again, and whatever inline layout holds the text. A selection
//! runs from its anchor, where it started, to its extent, where it ends, in
//! either order.
//!
//! The [`order`] and [`text`] modules find the selected parts of the inline
//! layouts of a fragment tree, and the selected text. The [`rects`] module
//! builds the rectangles painted behind it. Whether text can be
//! selected is resolved by the pre-paint step, see
//! [`LayoutInputData::selectable`](crate::prepaint::LayoutInputData::selectable).

pub mod order;
pub mod rects;
pub mod text;

use comet_layout::{
    fragment::{BoxFragmentKey, inline::InlineLayout, tree::FragmentTree},
    input::tree::LayoutInputTree,
};
use comet_tree::{
    node::{NodeKey, Text},
    ui::Ui,
};

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

/// A position in the text of a text node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextPosition {
    /// The text node.
    pub node: NodeKey,
    /// Byte offset into the [`Text`] of `node`, as written, before white
    /// space processing.
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

    /// The position in the text of the text node at `cursor` in `inline`.
    /// At the boundary of two text nodes, `side` picks the one before or
    /// after it, or by default the affinity of `cursor`. `None` if no text
    /// node is there.
    ///
    /// `inputs` is the input tree `inline` was built from, from the UI of
    /// `ui`.
    pub fn from_cursor(
        ui: &Ui,
        inputs: &LayoutInputTree,
        inline: &InlineLayout,
        cursor: parley::Cursor,
        side: Option<parley::Affinity>,
    ) -> Option<Self> {
        let item = inline.item_at(cursor.index(), side.unwrap_or(cursor.affinity()))?;
        let node = inputs.items.get(item)?.node;
        let text = ui.prop::<Text>(node)?;
        let index = inline.source_index(item, &text.0, cursor.index())?;

        Some(Self::new(node, index, cursor.affinity().into()))
    }
}

/// A selected range of text, from where it started to where it ends.
///
/// Both ends may be in different text nodes: the selection then covers the
/// text between them in tree order.
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

    /// Whether both ends are at the same position of the same text node.
    /// Ends in different nodes may still select no text, such as at the end
    /// of a node and the start of the next.
    pub fn is_collapsed(&self) -> bool {
        self.anchor.node == self.extent.node && self.anchor.index == self.extent.index
    }
}

/// The box owning the inline layout holding `position`, and the index of
/// `position` in its text. Text in the runs of a box split by blocks is
/// held by its run box.
///
/// `inputs` is the input tree `fragments` were built from, from the UI of
/// `ui`.
pub fn layout_position(
    ui: &Ui,
    inputs: &LayoutInputTree,
    fragments: &FragmentTree,
    position: TextPosition,
) -> Option<(BoxFragmentKey, usize)> {
    let item = inputs.item_for_node(position.node)?;
    let owner = fragments.inline_owner(inputs, item)?;
    let text = ui.prop::<Text>(position.node)?;
    let index = fragments
        .inline_layout(owner)?
        .layout_index(item, &text.0, position.index)?;

    Some((owner, index))
}
