pub mod cursor;

use core::ops::{Index, IndexMut};

use crate::tree::cursor::Cursor;
use slotmap::{Key, SlotMap};

pub struct SlotTree<K: Key, V> {
    arena: SlotMap<K, Node<K, V>>,
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
    pub fn cursor(&self, id: Option<K>) -> Cursor<K, V> {
        Cursor::new(self, id)
    }

    #[inline]
    /// Append child to parent node and return last parent node id
    pub fn append_child(&mut self, parent: K, id: K) -> Option<K> {
        let last_parent = self.remove_parent(id);
        let Some(parent_node) = self.arena.get_mut(parent) else {
            return last_parent;
        };

        if parent_node.first_child.is_none() {
            parent_node.first_child = Some(id);
        }

        let Some(prev_last_id) = parent_node.last_child.replace(id) else {
            return last_parent;
        };

        if let Some(prev_last_node) = self.arena.get_mut(prev_last_id) {
            prev_last_node.next_sibling = Some(id);
        };

        let Some(node) = self.arena.get_mut(id) else {
            return last_parent;
        };
        node.prev_sibling = Some(prev_last_id);

        last_parent
    }

    #[inline]
    /// Prepend child to parent node and return last parent node id
    pub fn prepend_child(&mut self, parent: K, id: K) -> Option<K> {
        let last_parent_id = self.remove_parent(id);
        let Some(parent_node) = self.arena.get_mut(parent) else {
            return last_parent_id;
        };

        if parent_node.last_child.is_none() {
            parent_node.last_child = Some(id);
        }

        let Some(prev_first_id) = parent_node.first_child.replace(id) else {
            return last_parent_id;
        };

        if let Some(prev_first_node) = self.arena.get_mut(prev_first_id) {
            prev_first_node.prev_sibling = Some(id);
        };

        let Some(node) = self.arena.get_mut(id) else {
            return last_parent_id;
        };
        node.next_sibling = Some(prev_first_id);

        last_parent_id
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
    pub fn delete_node(&mut self, id: K) {
        fn inner<K: Key, V>(tree: &mut SlotMap<K, Node<K, V>>, id: K) {
            let mut child = tree.remove(id).and_then(|node| node.first_child);
            while let Some(child_id) = child.take() {
                inner(tree, child_id);
                child = tree.remove(child_id).and_then(|node| node.next_sibling);
            }
        }

        inner(&mut self.arena, id);
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

pub struct Node<K: Key, T> {
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
