pub mod builder;
pub mod text;
pub mod tree;

use std::sync::Arc;

use bitflags::bitflags;
use comet_tree::node::NodeKey;
use slotmap::new_key_type;

pub use text::{SpanStyle, TextStyle, TextStyleBits};

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutInputItem {
    pub kind: LayoutInputKind,
    pub node: NodeKey,
    /// Inherited text properties. Equal styles share an allocation.
    pub text_style: Arc<TextStyle>,
    /// What the text style of the item, or of its descendants, depends on.
    pub bits: TextStyleBits,
    pub child_layout: ChildLayout,
    /// Changes the fragment builder has not laid out yet.
    pub flags: LayoutFlags,
    /// Source changes the input builder has not resolved yet.
    pub input_flags: InputFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutInputKind {
    Block,
    Inline,
    AtomicInline,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildLayout {
    // For Text and Content display node.
    None,
    Block,
    Inline,
    /// Independent direct child boxes arranged by a custom container algorithm.
    Container,
}

bitflags! {
    /// Invalidation state maintained by [`LayoutInputTree`] and consumed by the builder.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct LayoutFlags: u8 {
        /// At least one descendant needs refragmentation.
        const CHILD_NEED_LAYOUT = 1 << 0;

        /// Only the box style of this item changed, such as its size, padding,
        /// margin or insets. Its box is laid out again, keeping its shaped text
        /// and child fragments.
        const NEED_BOX_LAYOUT = 1 << 1;

        /// This item needs refragmentation: its content, child list, kind or
        /// text style changed. Includes [`Self::NEED_BOX_LAYOUT`], so check
        /// `contains(NEED_BOX_LAYOUT)` for any change of the box.
        const NEED_FULL_LAYOUT = 1 << 2 | Self::NEED_BOX_LAYOUT.bits();

        /// The item moved among the children of its parent, which is marked
        /// [`Self::NEED_FULL_LAYOUT`]. The fragment builder gives the box
        /// fragments of the item and of the items under it new keys, keeping
        /// their layout state, so later steps see them as new fragments in
        /// their new tree order. Alone, it lays nothing out again.
        const MOVED = 1 << 3;
    }
}

bitflags! {
    /// Source changes not resolved into the [`LayoutInputTree`](tree::LayoutInputTree)
    /// yet, set with [`LayoutInputTree::mark_input`](tree::LayoutInputTree::mark_input)
    /// and consumed by [`LayoutInputTreeBuilder::build`](builder::LayoutInputTreeBuilder::build).
    ///
    /// These are the inputs of the input step, while [`LayoutFlags`] are its
    /// outputs to the fragment builder. The builder clears the input flags of
    /// the items it visits.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct InputFlags: u8 {
        /// At least one descendant has input flags.
        const CHILD_NEED_INPUT = 1 << 0;

        /// A prop of the source node resolved into the item changed, such as
        /// an inherited text property or its display. The item is resolved
        /// again, and so are its descendants if its text style changed.
        const NEED_STYLE = 1 << 1;

        /// The child list of the source node changed: children were
        /// inserted, removed or moved. The item's children are reconciled
        /// with the source children.
        const NEED_CHILDREN = 1 << 2;
    }
}

new_key_type! {
    pub struct LayoutInputKey;
}
