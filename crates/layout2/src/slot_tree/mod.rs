pub mod iterator;

#[cfg(test)]
mod tests;

use core::ops::{Index, IndexMut};
use slotmap::{Key, SlotMap};

use self::iterator::Cursor;

/// An ordered forest backed by generational slots.
///
/// Nodes may also form parentless sibling chains. Mutations preserve subtrees.
/// Invalid keys, self-insertion, and moves that would create cycles are no-ops.
/// Insertion operations return the previous parent; `None` can mean either a
/// parentless node was moved or the operation was rejected.
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
        self.arena.insert(Node {
            parent: None,
            first_child: None,
            last_child: None,
            prev_sibling: None,
            next_sibling: None,
            data: value,
        })
    }

    /// Iterate from `start` through its following siblings, inclusively.
    /// Reversing this iterator visits that same range from its last sibling.
    pub fn cursor(&self, start: Option<K>) -> Cursor<'_, K, V> {
        Cursor::new(self, start)
    }

    pub fn children(&self, parent: K) -> Cursor<'_, K, V> {
        self.cursor(self.first_child(parent))
    }

    pub fn parent(&self, id: K) -> Option<K> {
        self.arena.get(id)?.parent
    }

    pub fn first_child(&self, id: K) -> Option<K> {
        self.arena.get(id)?.first_child
    }

    pub fn last_child(&self, id: K) -> Option<K> {
        self.arena.get(id)?.last_child
    }

    pub fn prev_sibling(&self, id: K) -> Option<K> {
        self.arena.get(id)?.prev_sibling
    }

    pub fn next_sibling(&self, id: K) -> Option<K> {
        self.arena.get(id)?.next_sibling
    }

    pub fn get(&self, id: K) -> Option<&V> {
        Some(&self.arena.get(id)?.data)
    }

    pub fn get_mut(&mut self, id: K) -> Option<&mut V> {
        Some(&mut self.arena.get_mut(id)?.data)
    }

    pub fn len(&self) -> usize {
        self.arena.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// Append `id` to `parent`, returning its previous parent.
    pub fn append(&mut self, parent: K, id: K) -> Option<K> {
        if !self.can_attach(id, Some(parent)) {
            return None;
        }

        let previous = self.remove_parent(id);
        let last = self.arena[parent].last_child;
        self.link(id, Some(parent), last, None);
        previous
    }

    /// Prepend `id` to `parent`, returning its previous parent.
    pub fn prepend(&mut self, parent: K, id: K) -> Option<K> {
        if !self.can_attach(id, Some(parent)) {
            return None;
        }

        let previous = self.remove_parent(id);
        let first = self.arena[parent].first_child;
        self.link(id, Some(parent), None, first);
        previous
    }

    /// Move `id` immediately before `target`, returning its previous parent.
    pub fn before(&mut self, target: K, id: K) -> Option<K> {
        let parent = self.arena.get(target)?.parent;
        if target == id || !self.can_attach(id, parent) {
            return None;
        }

        let previous = self.remove_parent(id);
        let prev = self.arena[target].prev_sibling;
        self.link(id, parent, prev, Some(target));
        previous
    }

    /// Move `id` immediately after `target`, returning its previous parent.
    pub fn after(&mut self, target: K, id: K) -> Option<K> {
        let parent = self.arena.get(target)?.parent;
        if target == id || !self.can_attach(id, parent) {
            return None;
        }

        let previous = self.remove_parent(id);
        let next = self.arena[target].next_sibling;
        self.link(id, parent, Some(target), next);
        previous
    }

    /// Detach a node from its parent and siblings, retaining its children.
    /// Also unlinks parentless siblings. Returns the previous parent.
    pub fn remove_parent(&mut self, id: K) -> Option<K> {
        let node = self.arena.get_mut(id)?;
        let parent = node.parent.take();
        let prev = node.prev_sibling.take();
        let next = node.next_sibling.take();

        if let Some(prev) = prev {
            self.arena[prev].next_sibling = next;
        }
        if let Some(next) = next {
            self.arena[next].prev_sibling = prev;
        }

        if let Some(parent) = parent {
            let node = &mut self.arena[parent];
            if node.first_child == Some(id) {
                node.first_child = next;
            }
            if node.last_child == Some(id) {
                node.last_child = prev;
            }
        }

        parent
    }

    /// Delete a node and all descendants without recursive stack growth.
    pub fn delete_node(&mut self, id: K) -> Option<V> {
        self.remove_parent(id);
        let root = self.arena.remove(id)?;
        let mut pending = Vec::new();
        pending.extend(root.first_child);

        while let Some(id) = pending.pop() {
            let node = self.arena.remove(id).expect("valid descendant");
            pending.extend(node.next_sibling);
            pending.extend(node.first_child);
        }

        Some(root.data)
    }

    pub fn clear(&mut self) {
        self.arena.clear();
    }

    fn can_attach(&self, id: K, mut parent: Option<K>) -> bool {
        if !self.arena.contains_key(id) {
            return false;
        }

        while let Some(key) = parent {
            if key == id {
                return false;
            }
            let Some(node) = self.arena.get(key) else {
                return false;
            };
            parent = node.parent;
        }

        true
    }

    /// Link a detached node into a validated gap.
    fn link(&mut self, id: K, parent: Option<K>, prev: Option<K>, next: Option<K>) {
        let node = &mut self.arena[id];
        node.parent = parent;
        node.prev_sibling = prev;
        node.next_sibling = next;

        if let Some(prev) = prev {
            self.arena[prev].next_sibling = Some(id);
        } else if let Some(parent) = parent {
            self.arena[parent].first_child = Some(id);
        }

        if let Some(next) = next {
            self.arena[next].prev_sibling = Some(id);
        } else if let Some(parent) = parent {
            self.arena[parent].last_child = Some(id);
        }
    }
}

impl<K: Key, V> Default for SlotTree<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Key, V> Index<K> for SlotTree<K, V> {
    type Output = V;

    fn index(&self, id: K) -> &Self::Output {
        &self.arena[id].data
    }
}

impl<K: Key, V> IndexMut<K> for SlotTree<K, V> {
    fn index_mut(&mut self, id: K) -> &mut Self::Output {
        &mut self.arena[id].data
    }
}

struct Node<K: Key, V> {
    parent: Option<K>,

    first_child: Option<K>,
    last_child: Option<K>,

    prev_sibling: Option<K>,
    next_sibling: Option<K>,

    data: V,
}
