//! The output of components.

use comet_tree::node::NodeKey;

/// Nodes built by a component, in order, not attached to any parent yet.
///
/// Anything iterating over [`NodeKey`]s is a view: a single key, an array,
/// a `Vec`, an `Option` or an iterator. [`NodeBuild::children`] appends the
/// nodes of the view its closure returns to the node being built.
///
/// [`NodeBuild::children`]: crate::NodeBuild::children
pub trait IntoView: IntoIterator<Item = NodeKey> {}

impl<T> IntoView for T where T: IntoIterator<Item = NodeKey> {}
