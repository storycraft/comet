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
    pub child_layout: ChildLayout,
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
    /// Changes of an item, set with
    /// [`DirtyMap::mark`](crate::dirty_map::DirtyMap::mark) and cleared
    /// with [`DirtyMap::clear`](crate::dirty_map::DirtyMap::clear)
    /// once every step consumed them.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct InputFlags: u8 {
        const NEED_LAYER = 1 << 0;

        /// The child list of the source node changed: children were
        /// inserted, removed or moved. The input builder reconciles the
        /// item's children with the source children.
        const NEED_CHILDREN = 1 << 1 | Self::NEED_LAYER.bits();

        const NEED_PRE_PAINT = 1 << 2;

        const NEED_BOX_LAYOUT = 1 << 3;

        /// A prop of the source node resolved into the item changed, or the
        /// item is new. The input builder resolves the item again, reconciles
        /// its children and marks every child with this flag. It marks the
        /// item [`Self::NEED_PRE_PAINT`] when its font size changed.
        const NEED_FULL_LAYOUT = 1 << 4
            | Self::NEED_BOX_LAYOUT.bits()
            | Self::NEED_CHILDREN.bits();
    }
}

new_key_type! {
    pub struct LayoutInputKey;
}
