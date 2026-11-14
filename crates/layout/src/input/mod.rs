pub mod builder;
pub mod text;
pub mod tree;

use std::sync::Arc;

use bitflags::bitflags;
use comet_tree::node::NodeKey;
use slotmap::new_key_type;

pub use text::{SpanStyle, TextStyle};

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
    }
}

new_key_type! {
    pub struct LayoutInputKey;
}
