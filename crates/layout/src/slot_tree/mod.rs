pub mod cursor;
#[cfg(test)]
mod tests;

use core::ops::{Index, IndexMut};
use slotmap::{Key, SlotMap};

use crate::slot_tree::cursor::Cursor;

pub struct SlotTree<K: Key, V> {
    arena: SlotMap<K, Node<K, V>>,
}

impl<K: Key, V> Default for SlotTree<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Key, V> SlotTree<K, V> {
    pub fn new() -> Self {
        Self {
            arena: SlotMap::with_key(),
        }
    }

    pub fn insert(&mut self, value: V) -> K {
        self.arena.insert(Node::new(value))
    }

    #[inline]
    /// Create a new [`Cursor`] for iterative traversal
    pub fn cursor(&'_ self, id: Option<K>) -> Cursor<'_, K, V> {
        Cursor::new(self, id)
    }

    #[inline]
    /// Append child to parent node and return last parent node id
    pub fn append(&mut self, parent: K, id: K) -> Option<K> {
        let parent_node = self.arena.get_mut(parent)?;
        match (parent_node.first_child, parent_node.last_child) {
            (_, Some(last_child)) => self.after(last_child, id),
            _ => {
                parent_node.first_child = Some(id);
                parent_node.last_child = Some(id);

                let last_parent = self.remove_parent(id);
                let Some(node) = self.arena.get_mut(id) else {
                    return last_parent;
                };
                node.parent = Some(parent);
                last_parent
            }
        }
    }

    #[inline]
    /// Prepend child to parent node and return last parent node id
    pub fn prepend(&mut self, parent: K, id: K) -> Option<K> {
        let parent_node = self.arena.get_mut(parent)?;
        match (parent_node.first_child, parent_node.last_child) {
            (Some(first_child), _) => self.before(first_child, id),
            _ => {
                parent_node.first_child = Some(id);
                parent_node.last_child = Some(id);

                let last_parent = self.remove_parent(id);
                let Some(node) = self.arena.get_mut(id) else {
                    return last_parent;
                };
                node.parent = Some(parent);
                last_parent
            }
        }
    }

    /// Insert a node before `target`. Returns previous parent id
    pub fn before(&mut self, target: K, id: K) -> Option<K> {
        fn inner<K: Key, V>(tree: &mut SlotTree<K, V>, target: K, id: K) -> Option<()> {
            let target_node = tree.arena.get_mut(target)?;
            let parent = target_node.parent;
            let prev_sibling = target_node.prev_sibling.replace(id);

            let node = tree.arena.get_mut(id)?;
            node.prev_sibling = prev_sibling;
            node.next_sibling = Some(target);
            node.parent = parent;

            match prev_sibling {
                Some(prev_sibling) => {
                    tree.arena.get_mut(prev_sibling)?.next_sibling = Some(id);
                }
                None => {
                    tree.arena.get_mut(parent?)?.first_child = Some(id);
                }
            }

            Some(())
        }

        let last_parent = self.remove_parent(id);
        inner(self, target, id);
        last_parent
    }

    /// Insert a node after `target`. Returns previous parent id
    pub fn after(&mut self, target: K, id: K) -> Option<K> {
        fn inner<K: Key, V>(tree: &mut SlotTree<K, V>, target: K, id: K) -> Option<()> {
            let target_node = tree.arena.get_mut(target)?;
            let parent = target_node.parent;
            let next_sibling = target_node.next_sibling.replace(id);

            let node = tree.arena.get_mut(id)?;
            node.prev_sibling = Some(target);
            node.next_sibling = next_sibling;
            node.parent = parent;

            match next_sibling {
                Some(next_sibling) => {
                    tree.arena.get_mut(next_sibling)?.prev_sibling = Some(id);
                }
                None => {
                    tree.arena.get_mut(parent?)?.last_child = Some(id);
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
    pub fn parent(&self, id: K) -> Option<K> {
        self.arena.get(id)?.parent
    }

    #[inline]
    /// Get first child node id
    pub fn first_child(&self, id: K) -> Option<K> {
        self.arena.get(id)?.first_child
    }

    #[inline]
    /// Get last child node id
    pub fn last_child(&self, id: K) -> Option<K> {
        self.arena.get(id)?.last_child
    }

    #[inline]
    /// Get next sibling node id
    pub fn next_sibling(&self, id: K) -> Option<K> {
        self.arena.get(id)?.next_sibling
    }

    #[inline]
    /// Get previous sibling node id
    pub fn prev_sibling(&self, id: K) -> Option<K> {
        self.arena.get(id)?.prev_sibling
    }

    #[inline]
    /// Get associated node data
    pub fn get(&self, id: K) -> Option<&V> {
        Some(&self.arena.get(id)?.data)
    }

    #[inline]
    /// Get mutable access to associated node data
    pub fn get_mut(&mut self, id: K) -> Option<&mut V> {
        Some(&mut self.arena.get_mut(id)?.data)
    }

    /// Disconnect node from the parent and return the parent node id
    pub fn remove_parent(&mut self, id: K) -> Option<K> {
        let node = self.arena.get_mut(id)?;
        let parent_id = node.parent.take()?;
        let prev_sibling_id = node.prev_sibling.take();
        let next_sibling_id = node.next_sibling.take();

        if let Some(prev_sibling_node) =
            prev_sibling_id.and_then(|prev_sibling_id| self.arena.get_mut(prev_sibling_id))
        {
            prev_sibling_node.next_sibling = next_sibling_id;
        };

        if let Some(next_sibling_node) =
            next_sibling_id.and_then(|next_sibling_id| self.arena.get_mut(next_sibling_id))
        {
            next_sibling_node.prev_sibling = next_sibling_id;
        };

        if let Some(parent_node) = self.arena.get_mut(parent_id) {
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
    pub fn delete_node(&mut self, id: K) -> Option<V> {
        self.remove_parent(id);
        let node = self.arena.remove(id)?;
        let mut child = node.first_child;
        while let Some(child_id) = child.take() {
            self.delete_node(child_id);
            child = self
                .arena
                .remove(child_id)
                .and_then(|node| node.next_sibling);
        }

        Some(node.data)
    }

    #[inline]
    /// Clear the entire tree
    pub fn clear(&mut self) {
        self.arena.clear();
    }
}

impl<K: Key, V> Index<K> for SlotTree<K, V> {
    type Output = V;

    fn index(&self, index: K) -> &Self::Output {
        &self.arena[index].data
    }
}

impl<K: Key, V> IndexMut<K> for SlotTree<K, V> {
    fn index_mut(&mut self, index: K) -> &mut Self::Output {
        &mut self.arena[index].data
    }
}

struct Node<K: Key, T> {
    parent: Option<K>,

    first_child: Option<K>,
    last_child: Option<K>,

    prev_sibling: Option<K>,
    next_sibling: Option<K>,

    data: T,
}

impl<K: Key, T> Node<K, T> {
    const fn new(data: T) -> Self {
        Self {
            parent: None,

            first_child: None,
            last_child: None,

            prev_sibling: None,
            next_sibling: None,

            data,
        }
    }
}
