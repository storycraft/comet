pub mod cursor;

#[cfg(test)]
mod tests;

use hecs::{Component, DynamicBundle, Entity, EntityBuilder, EntityRef, Ref, RefMut, World};

use crate::tree::cursor::Cursor;

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

    pub fn spawn(&mut self, bundle: impl DynamicBundle + Send + Sync) -> Entity {
        self.world
            .spawn(self.builder.add(Node::new()).add_bundle(bundle).build())
    }

    pub fn components(&'_ self, key: Entity) -> Option<EntityRef<'_>> {
        self.world.entity(key).ok()
    }

    pub fn add_components(&mut self, key: Entity, components: impl DynamicBundle) {
        _ = self.world.insert(key, components);
    }

    pub fn remove_component<T: Component>(&mut self, key: Entity) -> Option<T> {
        self.world.remove_one::<T>(key).ok()
    }

    #[inline]
    /// Create a new [`Cursor`] for iterative traversal
    pub fn cursor(&'_ self, id: Option<Entity>) -> Cursor<'_> {
        Cursor::new(self, id)
    }

    /// Append child to parent node and return last parent node id
    pub fn append(&mut self, parent: Entity, id: Entity) -> Option<Entity> {
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
    pub fn prepend(&mut self, parent: Entity, id: Entity) -> Option<Entity> {
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
    pub fn before(&mut self, target: Entity, id: Entity) -> Option<Entity> {
        fn inner(tree: &mut ArchetypalTree, target: Entity, id: Entity) -> Option<()> {
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
    pub fn after(&mut self, target: Entity, id: Entity) -> Option<Entity> {
        fn inner(tree: &mut ArchetypalTree, target: Entity, id: Entity) -> Option<()> {
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
    pub fn parent(&self, id: Entity) -> Option<Entity> {
        self.world.node(id)?.parent
    }

    #[inline]
    /// Get first child node id
    pub fn first_child(&self, id: Entity) -> Option<Entity> {
        self.world.node(id)?.first_child
    }

    #[inline]
    /// Get last child node id
    pub fn last_child(&self, id: Entity) -> Option<Entity> {
        self.world.node(id)?.last_child
    }

    #[inline]
    /// Get next sibling node id
    pub fn next_sibling(&self, id: Entity) -> Option<Entity> {
        self.world.node(id)?.next_sibling
    }

    #[inline]
    /// Get previous sibling node id
    pub fn prev_sibling(&self, id: Entity) -> Option<Entity> {
        self.world.node(id)?.prev_sibling
    }

    /// Disconnect node from the parent and return the parent node id
    pub fn remove_parent(&mut self, id: Entity) -> Option<Entity> {
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

    /// Delete node recursively
    pub fn delete(&mut self, id: Entity) {
        fn inner(tree: &mut ArchetypalTree, id: Entity) {
            let mut child = tree.world.node(id).and_then(|node| node.first_child);
            _ = tree.world.despawn(id);
            while let Some(child_id) = child.take() {
                inner(tree, child_id);
                child = tree.world.node(id).and_then(|node| node.first_child);
                _ = tree.world.despawn(child_id);
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

struct Node {
    parent: Option<Entity>,

    first_child: Option<Entity>,
    last_child: Option<Entity>,

    prev_sibling: Option<Entity>,
    next_sibling: Option<Entity>,
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

#[extend::ext]
impl World {
    #[inline]
    fn node(&'_ self, id: Entity) -> Option<Ref<'_, Node>> {
        self.get::<&Node>(id).ok()
    }

    #[inline]
    fn node_mut(&'_ self, id: Entity) -> Option<RefMut<'_, Node>> {
        self.get::<&mut Node>(id).ok()
    }
}
