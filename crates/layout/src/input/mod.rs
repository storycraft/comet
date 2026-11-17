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
    /// Union of the [`Self::flags2`] of every descendant. Kept on the
    /// ancestors of an item by [`LayoutInputTree::mark`](tree::LayoutInputTree::mark),
    /// so subtrees without the flags are skipped.
    pub child_flags: InputFlags2,
    /// Changes of this item not consumed yet.
    pub flags2: InputFlags2,
    pub kind: LayoutInputKind,
    pub node: NodeKey,
    /// Inherited text properties. Equal styles share an allocation.
    pub text_style: Arc<TextStyle>,
    pub child_layout: ChildLayout,
    /// Changes the fragment builder has not laid out yet. No longer written
    /// by the input module; replaced by [`Self::flags2`].
    pub flags: LayoutFlags,
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
    /// Changes of an item, set with
    /// [`LayoutInputTree::mark`](tree::LayoutInputTree::mark) and cleared
    /// with [`LayoutInputTree::clear_flags`](tree::LayoutInputTree::clear_flags)
    /// once every step consumed them.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct InputFlags2: u8 {
        /// The child list of the source node changed: children were
        /// inserted, removed or moved. The input builder reconciles the
        /// item's children with the source children.
        const NEED_CHILDREN = 1 << 0;

        const NEED_PRE_PAINT = 1 << 1;

        const NEED_BOX_LAYOUT = 1 << 2 | Self::NEED_PRE_PAINT.bits();

        /// A prop of the source node resolved into the item changed, or the
        /// item is new. The input builder resolves the item again, reconciles
        /// its children and marks every child with this flag.
        const NEED_FULL_LAYOUT = 1 << 3 | Self::NEED_BOX_LAYOUT.bits() | Self::NEED_CHILDREN.bits();
    }
}

new_key_type! {
    pub struct LayoutInputKey;
}
