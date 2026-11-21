//! Accessibility trees for comet UIs, built with [`accesskit`].
//!
//! An [`A11yTree`] follows the layout input tree in the order of the UI, with
//! a node for each fragment of each item, and is kept up to date by an
//! [`A11yTreeBuilder`],
//! which records the fragments whose nodes changed. Take their nodes for
//! an [`accesskit::TreeUpdate`] with [`A11yTree::take_update`].

pub mod builder;
pub mod style;
pub mod tree;

use bitflags::bitflags;

pub use builder::A11yTreeBuilder;
pub use tree::{A11yNode, A11yTree};

bitflags! {
    /// Accessibility changes of a layout input item, set with
    /// [`A11yTree::mark`] for
    /// changes layout flags do not carry, such as a changed
    /// [`Role`](crate::style::Role). Cleared by the builder once resolved.
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct A11yFlags: u8 {
        /// The nodes of the item are resolved again.
        const DIRTY = 1 << 0;
        /// A descendant of the item is [`Self::DIRTY`].
        const CHILD_DIRTY = 1 << 1;
    }
}
