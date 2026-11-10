pub mod builder;
pub mod tree;

use std::sync::Arc;

use bitflags::bitflags;
use comet_tree::node::NodeKey;
use slotmap::new_key_type;

use crate::parley::TextStyle;

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutInputItem {
    pub kind: LayoutInputKind,
    pub node: NodeKey,
    /// Inherited text properties. Equal styles share an allocation.
    pub text_style: Arc<TextStyle>,
    pub child_layout: ChildLayout,
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
        /// This item needs refragmentation.
        const DIRTY = 1 << 0;

        /// At least one descendant needs refragmentation.
        const CHILDREN_DIRTY = 1 << 1;
    }
}

new_key_type! {
    pub struct LayoutInputKey;
}
