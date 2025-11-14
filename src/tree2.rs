pub mod cursor;
#[cfg(test)]
mod tests;

use crate::tree2::cursor::Cursor;
use core::num::NonZeroU64;
use hecs::{Component, ComponentRef, DynamicBundle, Entity, EntityBuilder, EntityRef, World};

pub struct ArchetypalTree {
    world: World,
    builder: EntityBuilder,
}

impl ArchetypalTree {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            builder: EntityBuilder::new(),
        }
    }

    pub fn spawn(&mut self, bundle: impl DynamicBundle + Send + Sync) -> EntityId {
        EntityId(
            self.world
                .spawn(self.builder.add(Node::new()).add_bundle(bundle).build()),
        )
    }

    pub fn components(&self, key: EntityId) -> Option<Components> {
        Some(Components(self.world.entity(key.0).ok()?))
    }

    #[inline]
    pub fn get<'a, T: ComponentRef<'a>>(&'a self, key: EntityId) -> Option<T::Ref> {
        self.components(key)?.get::<T>()
    }

    pub fn add_components(&mut self, key: EntityId, components: impl DynamicBundle) {
        _ = self.world.insert(key.0, components);
    }

    pub fn remove_component<T: Component>(&mut self, key: EntityId) -> Option<T> {
        self.world.remove_one::<T>(key.0).ok()
    }

    #[inline]
    /// Create a new [`Cursor`] for iterative traversal
    pub fn cursor(&self, id: Option<EntityId>) -> Cursor {
        Cursor::new(self, id)
    }

    /// Append child to parent node and return last parent node id
    pub fn append(&mut self, parent: EntityId, id: EntityId) -> Option<EntityId> {
        let mut parent_node = self.world.node_mut(parent)?;
        match (parent_node.first_child, parent_node.last_child) {
            (_, Some(last_child)) => {
                drop(parent_node);
                self.after(last_child, id)
            }
            _ => {
                parent_node.first_child = Some(id);
                parent_node.last_child = Some(id);
                drop(parent_node);
                let last_parent = self.remove_parent(id);
                let Some(mut node) = self.world.node_mut(id) else {
                    return last_parent;
                };
                node.parent = Some(parent);
                last_parent
            }
        }
    }

    #[inline]
    /// Prepend child to parent node and return last parent node id
    pub fn prepend(&mut self, parent: EntityId, id: EntityId) -> Option<EntityId> {
        let mut parent_node = self.world.node_mut(id)?;
        match (parent_node.first_child, parent_node.last_child) {
            (Some(first_child), _) => {
                drop(parent_node);
                self.before(first_child, id)
            }
            _ => {
                parent_node.first_child = Some(id);
                parent_node.last_child = Some(id);
                drop(parent_node);

                let last_parent = self.remove_parent(id);
                let Some(mut node) = self.world.node_mut(id) else {
                    return last_parent;
                };
                node.parent = Some(parent);
                last_parent
            }
        }
    }

    /// Insert a node before `target`. Returns previous parent id
    pub fn before(&mut self, target: EntityId, id: EntityId) -> Option<EntityId> {
        fn inner(tree: &mut ArchetypalTree, target: EntityId, id: EntityId) -> Option<()> {
            let mut target_node = tree.world.node_mut(target)?;
            let parent = target_node.parent;
            let prev_sibling = target_node.prev_sibling.replace(id);
            drop(target_node);

            let mut node = tree.world.node_mut(id)?;
            node.prev_sibling = prev_sibling;
            node.next_sibling = Some(target);
            node.parent = parent;
            drop(node);

            match prev_sibling {
                Some(prev_sibling) => {
                    tree.world.node_mut(prev_sibling)?.next_sibling = Some(id);
                }
                None => {
                    tree.world.node_mut(parent?)?.first_child = Some(id);
                }
            }
            Some(())
        }

        let last_parent = self.remove_parent(id);
        inner(self, target, id);
        last_parent
    }

    /// Insert a node after `target`. Returns previous parent id
    pub fn after(&mut self, target: EntityId, id: EntityId) -> Option<EntityId> {
        fn inner(tree: &mut ArchetypalTree, target: EntityId, id: EntityId) -> Option<()> {
            let mut target_node = tree.world.node_mut(target)?;
            let parent = target_node.parent;
            let next_sibling = target_node.next_sibling.replace(id);
            drop(target_node);

            let mut node = tree.world.node_mut(id)?;
            node.prev_sibling = Some(target);
            node.next_sibling = next_sibling;
            node.parent = parent;
            drop(node);

            match next_sibling {
                Some(next_sibling) => {
                    tree.world.node_mut(next_sibling)?.prev_sibling = Some(id);
                }
                None => {
                    tree.world.node_mut(parent?)?.last_child = Some(id);
                }
            }

            Some(())
        }

        let last_parent = self.remove_parent(id);
        inner(self, target, id);
        last_parent
    }

    #[inline]
    /// Get parent node id
    pub fn parent(&self, id: EntityId) -> Option<EntityId> {
        self.world.node(id)?.parent
    }

    #[inline]
    /// Get first child node id
    pub fn first_child(&self, id: EntityId) -> Option<EntityId> {
        self.world.node(id)?.first_child
    }

    #[inline]
    /// Get last child node id
    pub fn last_child(&self, id: EntityId) -> Option<EntityId> {
        self.world.node(id)?.last_child
    }

    #[inline]
    /// Get next sibling node id
    pub fn next_sibling(&self, id: EntityId) -> Option<EntityId> {
        self.world.node(id)?.next_sibling
    }

    #[inline]
    /// Get previous sibling node id
    pub fn prev_sibling(&self, id: EntityId) -> Option<EntityId> {
        self.world.node(id)?.prev_sibling
    }

    /// Disconnect node from the parent and return the parent node id
    pub fn remove_parent(&mut self, id: EntityId) -> Option<EntityId> {
        let mut node = self.world.node_mut(id)?;
        let parent_id = node.parent.take()?;
        let prev_sibling_id = node.prev_sibling.take();
        let next_sibling_id = node.next_sibling.take();
        drop(node);

        if let Some(mut prev_sibling_node) =
            prev_sibling_id.and_then(|prev_sibling_id| self.world.node_mut(prev_sibling_id))
        {
            prev_sibling_node.next_sibling = next_sibling_id;
        };

        if let Some(mut next_sibling_node) =
            next_sibling_id.and_then(|next_sibling_id| self.world.node_mut(next_sibling_id))
        {
            next_sibling_node.prev_sibling = next_sibling_id;
        };

        if let Some(mut parent_node) = self.world.node_mut(parent_id) {
            if parent_node.first_child == Some(id) {
                parent_node.first_child = None;
            }

            if parent_node.last_child == Some(id) {
                parent_node.last_child = None;
            }
        };

        Some(parent_id)
    }

    /// Delete node including and its children
    pub fn delete(&mut self, id: EntityId) {
        fn inner(tree: &mut ArchetypalTree, id: EntityId) {
            let mut child = tree.world.node(id).and_then(|node| node.first_child);
            _ = tree.world.despawn(id.0);
            while let Some(child_id) = child.take() {
                inner(tree, child_id);
                child = tree.world.node(id).and_then(|node| node.first_child);
                _ = tree.world.despawn(child_id.0);
            }
        }

        inner(self, id);
    }

    #[inline]
    /// Clear the entire tree
    pub fn clear(&mut self) {
        self.world.clear();
    }
}

impl Default for ArchetypalTree {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct EntityId(Entity);

impl EntityId {
    #[inline]
    pub const fn bits(self) -> NonZeroU64 {
        self.0.to_bits()
    }

    #[inline]
    pub const fn from_bits(bits: u64) -> Option<Self> {
        if let Some(entity) = Entity::from_bits(bits) {
            Some(Self(entity))
        } else {
            None
        }
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self(Entity::DANGLING)
    }
}

struct Node {
    parent: Option<EntityId>,

    first_child: Option<EntityId>,
    last_child: Option<EntityId>,

    prev_sibling: Option<EntityId>,
    next_sibling: Option<EntityId>,
}

impl Node {
    const fn new() -> Self {
        Self {
            parent: None,

            first_child: None,
            last_child: None,

            prev_sibling: None,
            next_sibling: None,
        }
    }
}

pub struct Components<'a>(EntityRef<'a>);

impl<'a> Components<'a> {
    #[inline]
    pub fn get<T: ComponentRef<'a>>(&self) -> Option<T::Ref> {
        self.0.get::<T>()
    }
}

#[extend::ext]
impl World {
    #[inline]
    fn node(&self, id: EntityId) -> Option<<&Node as ComponentRef>::Ref> {
        self.get::<&Node>(id.0).ok()
    }

    #[inline]
    fn node_mut(&self, id: EntityId) -> Option<<&mut Node as ComponentRef>::Ref> {
        self.get::<&mut Node>(id.0).ok()
    }
}
