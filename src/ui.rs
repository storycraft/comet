pub mod cursor;

use crate::{
    tree2::{Components, EntityId, EntityTree},
    ui::cursor::Cursor,
};
use hecs::{Component, ComponentRef, DynamicBundle, EntityBuilder, Ref, RefMut};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(transparent)]
pub struct NodeKey(EntityId);

#[derive(Debug)]
pub enum Node {
    Div,
    Text(String),
}

#[non_exhaustive]
pub struct Ui {
    inner: EntityTree,
    builder: EntityBuilder,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            inner: EntityTree::new(),
            builder: EntityBuilder::new(),
        }
    }

    #[inline]
    pub fn create_node<Props>(&mut self, node: Node, props: Props) -> NodeKey
    where
        Props: DynamicBundle + Send + Sync + 'static,
    {
        NodeKey(
            self.inner.spawn(
                self.builder
                    .add(NodeWrapper(node))
                    .add_bundle(props)
                    .build(),
            ),
        )
    }

    #[inline]
    pub fn props(&self, key: NodeKey) -> Option<Components> {
        self.inner.components(key.0)
    }

    #[inline]
    pub fn prop<'a, T: ComponentRef<'a>>(&'a self, key: NodeKey) -> Option<T::Ref> {
        self.props(key)?.get::<T>()
    }

    #[inline]
    pub fn node(&self, key: NodeKey) -> Option<Ref<Node>> {
        Some(Ref::map(self.prop::<&NodeWrapper>(key)?, |v| &v.0))
    }

    #[inline]
    pub fn node_mut(&self, key: NodeKey) -> Option<RefMut<Node>> {
        Some(RefMut::map(self.prop::<&mut NodeWrapper>(key)?, |v| {
            &mut v.0
        }))
    }

    #[inline]
    pub fn add_props(&mut self, key: NodeKey, props: impl DynamicBundle) {
        self.inner.add_components(key.0, props);
    }

    #[inline]
    pub fn remove_prop<T: Component>(&mut self, key: NodeKey) -> Option<T> {
        self.inner.remove_component::<T>(key.0)
    }

    #[inline]
    /// Create a new [`Cursor`] for iterative traversal
    pub fn cursor(&self, id: Option<NodeKey>) -> Cursor {
        Cursor(self.inner.cursor(id.map(|v| v.0)))
    }

    #[inline]
    /// Append child to parent node and return last parent node id
    pub fn append(&mut self, parent: NodeKey, id: NodeKey) -> Option<NodeKey> {
        Some(NodeKey(self.inner.append(parent.0, id.0)?))
    }

    #[inline]
    /// Prepend child to parent node and return last parent node id
    pub fn prepend(&mut self, parent: NodeKey, id: NodeKey) -> Option<NodeKey> {
        Some(NodeKey(self.inner.prepend(parent.0, id.0)?))
    }

    #[inline]
    /// Insert a node before `target`. Returns previous parent id
    pub fn before(&mut self, target: NodeKey, id: NodeKey) -> Option<NodeKey> {
        Some(NodeKey(self.inner.before(target.0, id.0)?))
    }

    #[inline]
    /// Insert a node after `target`. Returns previous parent id
    pub fn after(&mut self, target: NodeKey, id: NodeKey) -> Option<NodeKey> {
        Some(NodeKey(self.inner.after(target.0, id.0)?))
    }

    #[inline]
    /// Get parent node id
    pub fn parent(&self, id: NodeKey) -> Option<NodeKey> {
        Some(NodeKey(self.inner.parent(id.0)?))
    }

    #[inline]
    /// Get first child node id
    pub fn first_child(&self, id: NodeKey) -> Option<NodeKey> {
        Some(NodeKey(self.inner.first_child(id.0)?))
    }

    #[inline]
    /// Get last child node id
    pub fn last_child(&self, id: NodeKey) -> Option<NodeKey> {
        Some(NodeKey(self.inner.last_child(id.0)?))
    }

    #[inline]
    /// Get next sibling node id
    pub fn next_sibling(&self, id: NodeKey) -> Option<NodeKey> {
        Some(NodeKey(self.inner.next_sibling(id.0)?))
    }

    #[inline]
    /// Get previous sibling node id
    pub fn prev_sibling(&self, id: NodeKey) -> Option<NodeKey> {
        Some(NodeKey(self.inner.prev_sibling(id.0)?))
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

struct NodeWrapper(Node);
