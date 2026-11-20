//! Building nodes ahead of inserting them into a [`Ui`](crate::ui::Ui).
//!
//! Adding or removing props of a node already in a `Ui` moves it between
//! archetypes, which is relatively expensive. Collect every prop with a
//! [`NodeBuilder`] first, then insert the [`BuiltNode`] at once with
//! [`Ui::create_prebuilt`](crate::ui::Ui::create_prebuilt).

use hecs::{BuiltEntity, DynamicBundle, EntityBuilder};

use crate::{
    node::{Node, NodeKey},
    style::StyleProp,
    ui::UiNode,
};

/// Collects props of a node before it is inserted. Reusable: building or
/// [clearing](Self::clear) empties it while keeping its allocation.
#[derive(Default)]
pub struct NodeBuilder(EntityBuilder);

impl NodeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a prop, replacing an earlier one of the same type.
    pub fn add<T: StyleProp>(&mut self, prop: T) -> &mut Self {
        self.0.add(prop);
        self
    }

    pub(crate) fn add_bundle(&mut self, props: impl DynamicBundle) -> &mut Self {
        self.0.add_bundle(props);
        self
    }

    /// Whether a prop of type `T` was added.
    pub fn has<T: StyleProp>(&self) -> bool {
        self.0.has::<T>()
    }

    /// Drop every added prop.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Finish a node of `kind` with the added props. The builder is empty
    /// again once the built node is inserted or dropped.
    pub fn build(&mut self, kind: Node) -> BuiltNode<'_> {
        self.0.add(UiNode::new(kind));
        BuiltNode(self.0.build())
    }
}

/// A node built by a [`NodeBuilder`], ready to insert with
/// [`Ui::create_prebuilt`](crate::ui::Ui::create_prebuilt).
pub struct BuiltNode<'a>(pub(crate) BuiltEntity<'a>);

/// A key reserved with [`Ui::reserve_node`](crate::ui::Ui::reserve_node) for
/// a node inserted later with
/// [`Ui::create_prebuilt`](crate::ui::Ui::create_prebuilt). It has no props
/// until then, so `Ui` treats its key as an invalid node.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ReservedNodeKey(pub(crate) NodeKey);

impl ReservedNodeKey {
    /// The key the node will have once created.
    pub const fn key(&self) -> NodeKey {
        self.0
    }
}
