pub mod cursor;
pub mod layout;

use crate::{
    style::{PropLevel, StyleProp, StyleProps},
    tree::archetypal::{ArchetypalTree, Components},
    ui::cursor::Cursor,
};
use hecs::{DynamicBundle, Entity, EntityBuilder, Ref, RefMut};

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct NodeKey(Entity);

impl NodeKey {
    #[inline]
    pub const fn id(self) -> u32 {
        self.0.id()
    }
}

impl Default for NodeKey {
    fn default() -> Self {
        Self(Entity::DANGLING)
    }
}

#[derive(Debug)]
pub enum Node {
    Div,
    Text(String),
}

#[non_exhaustive]
pub struct Ui {
    inner: ArchetypalTree,
    builder: EntityBuilder,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            inner: ArchetypalTree::new(),
            builder: EntityBuilder::new(),
        }
    }

    #[inline]
    pub fn create_node(&mut self, node: Node, props: impl StyleProps) -> NodeKey {
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
    pub fn props(&'_ self, key: NodeKey) -> Option<Props<'_>> {
        Some(Props(self.inner.components(key.0)?))
    }

    #[inline]
    pub fn prop<T: StyleProp>(&'_ self, key: NodeKey) -> Option<Ref<'_, T>> {
        self.props(key)?.get::<T>()
    }

    #[inline]
    pub fn prop_mut<T: StyleProp>(&'_ self, key: NodeKey) -> Option<RefMut<'_, T>> {
        self.props(key)?.get_mut::<T>()
    }

    #[inline]
    pub fn node(&'_ self, key: NodeKey) -> Option<Ref<'_, Node>> {
        Some(Ref::map(self.prop::<NodeWrapper>(key)?, |v| &v.0))
    }

    #[inline]
    pub fn node_mut(&'_ self, key: NodeKey) -> Option<RefMut<'_, Node>> {
        Some(RefMut::map(self.prop_mut::<NodeWrapper>(key)?, |v| {
            &mut v.0
        }))
    }

    #[inline]
    pub fn set_props(&mut self, key: NodeKey, props: impl DynamicBundle) {
        self.inner.add_components(key.0, props);
    }

    #[inline]
    pub fn remove_prop<T: StyleProp>(&mut self, key: NodeKey) -> Option<T> {
        self.inner.remove_component::<T>(key.0)
    }

    #[inline]
    /// Create a new [`Cursor`] for iterative traversal
    pub fn cursor(&'_ self, id: Option<NodeKey>) -> Cursor<'_> {
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

impl StyleProp for NodeWrapper {
    const LEVEL: PropLevel = PropLevel::Layout;
}

pub struct Props<'a>(Components<'a>);

impl<'a> Props<'a> {
    #[inline]
    pub fn get<T: StyleProp>(&self) -> Option<Ref<'a, T>> {
        self.0.get::<&T>()
    }

    #[inline]
    pub fn get_mut<T: StyleProp>(&self) -> Option<RefMut<'a, T>> {
        self.0.get::<&mut T>()
    }
}
