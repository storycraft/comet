//! Accessibility trees for comet UIs, built with [`accesskit`].
//!
//! An [`AxTree`] follows the layout input tree in the order of the UI, with
//! a node for each item and a text run node for each glyph run of a text,
//! and is kept up to date by an [`AxTreeBuilder`], which records the nodes
//! that changed. Take their nodes for
//! an [`accesskit::TreeUpdate`] with [`AxTree::take_update`].

pub mod builder;
pub mod style;
pub mod tree;

use accesskit::NodeId;
use bitflags::bitflags;
use comet_layout::input::LayoutInputKey;
use slotmap::{Key, KeyData, new_key_type};

pub use builder::AxTreeBuilder;
pub use tree::AxTree;

bitflags! {
    /// Accessibility changes of a layout input item, set with
    /// [`AxTree::mark`] for
    /// changes layout flags do not carry, such as a changed
    /// [`Role`](crate::style::Role). Cleared by the builder once resolved.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct AxFlags: u8 {
        /// The nodes of the item are resolved again.
        const DIRTY = 1 << 0;
        /// A descendant of the item is [`Self::DIRTY`].
        const CHILD_DIRTY = 1 << 1;
    }
}

new_key_type! {
    /// Key of a node in an [`AxTree`], and its [`NodeId`].
    pub struct AxKey;
}

/// What a node of an [`AxTree`] stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxNodeKind {
    /// A layout input item.
    Item,
    /// A glyph run of a text item, as a [`TextRun`](accesskit::Role::TextRun)
    /// inside the item's node.
    TextRun,
}

/// One node of an [`AxTree`].
#[derive(Debug, Clone, PartialEq)]
pub struct AxNode {
    /// The node last resolved.
    pub node: accesskit::Node,
    /// The layout input item it belongs to.
    pub item: LayoutInputKey,
    pub kind: AxNodeKind,
    /// Whether the node was resolved. New nodes are listed by their parent
    /// before they are resolved themselves.
    pub(crate) resolved: bool,
}

/// The node id of a node.
pub fn node_id(key: AxKey) -> NodeId {
    NodeId(key.data().as_ffi())
}

/// The key of a node id, such as the target of an action request.
pub fn ax_key(id: NodeId) -> AxKey {
    KeyData::from_ffi(id.0).into()
}
