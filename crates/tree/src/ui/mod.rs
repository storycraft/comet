//! An archetypal UI forest. Every element has a node kind and private links.
//!
//! Insertion methods return the old parent. Invalid keys, cycles, and sibling
//! insertion beside a detached root are no-ops. `None` therefore means either
//! no previous parent or an invalid operation. Inserting relative to oneself
//! is a no-op. Keys belong to the `Ui` that created them.
pub mod cursor;

use hecs::{Entity, EntityBuilder, EntityRef, Ref, RefMut, World};

use crate::{
    node::{Node, NodeKey},
    style::{StyleProp, StyleProps},
};

use self::cursor::Cursor;

#[derive(Default)]
pub struct Ui {
    world: World,
    builder: EntityBuilder,
}

impl Ui {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(&mut self, node: Node, props: impl StyleProps) -> NodeKey {
        NodeKey(
            self.world.spawn(
                self.builder
                    .add(Relation::default())
                    .add(Kind(node))
                    .add_bundle(props)
                    .build(),
            ),
        )
    }

    pub fn node(&self, key: NodeKey) -> Option<Ref<'_, Node>> {
        Some(Ref::map(self.world.get::<&Kind>(key.0).ok()?, |kind| {
            &kind.0
        }))
    }

    pub fn props(&self, key: NodeKey) -> Option<Props<'_>> {
        Some(Props(self.world.entity(key.0).ok()?))
    }

    pub fn prop<T: StyleProp>(&self, key: NodeKey) -> Option<Ref<'_, T>> {
        self.props(key)?.get::<T>()
    }

    /// Uses hecs runtime borrow checking; overlapping incompatible borrows panic.
    pub fn prop_mut<T: StyleProp>(&self, key: NodeKey) -> Option<RefMut<'_, T>> {
        self.props(key)?.get_mut::<T>()
    }

    /// Insert or replace properties. A nonexistent key is ignored.
    pub fn set_props(&mut self, key: NodeKey, props: impl StyleProps) {
        let _ = self.world.insert(key.0, props);
    }

    pub fn remove_prop<T: StyleProp>(&mut self, key: NodeKey) -> Option<T> {
        self.world.remove_one::<T>(key.0).ok()
    }

    fn links(&self, id: Entity) -> Option<Relation> {
        self.world
            .get::<&Relation>(id)
            .ok()
            .map(|relation| *relation)
    }

    fn edit(&mut self, id: Entity, f: impl FnOnce(&mut Relation)) -> Option<()> {
        let mut relation = self.world.get::<&mut Relation>(id).ok()?;
        f(&mut relation);

        Some(())
    }

    pub fn parent(&self, id: NodeKey) -> Option<NodeKey> {
        self.links(id.0)?.parent.map(NodeKey)
    }

    pub fn first_child(&self, id: NodeKey) -> Option<NodeKey> {
        self.links(id.0)?.first_child.map(NodeKey)
    }

    pub fn last_child(&self, id: NodeKey) -> Option<NodeKey> {
        self.links(id.0)?.last_child.map(NodeKey)
    }

    pub fn next_sibling(&self, id: NodeKey) -> Option<NodeKey> {
        self.links(id.0)?.next_sibling.map(NodeKey)
    }

    pub fn prev_sibling(&self, id: NodeKey) -> Option<NodeKey> {
        self.links(id.0)?.prev_sibling.map(NodeKey)
    }

    fn can_attach(&self, parent: Entity, id: Entity) -> bool {
        if self.links(id).is_none() {
            return false;
        }

        let mut current = Some(parent);
        while let Some(ancestor) = current {
            if ancestor == id {
                return false;
            }

            let Some(relation) = self.links(ancestor) else {
                return false;
            };

            current = relation.parent;
        }

        true
    }

    /// Detach a subtree without deleting its elements.
    pub fn remove_parent(&mut self, id: NodeKey) -> Option<NodeKey> {
        let relation = self.links(id.0)?;
        let parent = relation.parent?;

        if let Some(prev) = relation.prev_sibling {
            self.edit(prev, |n| n.next_sibling = relation.next_sibling)?;
        } else {
            self.edit(parent, |n| n.first_child = relation.next_sibling)?;
        }

        if let Some(next) = relation.next_sibling {
            self.edit(next, |n| n.prev_sibling = relation.prev_sibling)?;
        } else {
            self.edit(parent, |n| n.last_child = relation.prev_sibling)?;
        }

        self.edit(id.0, |n| {
            n.parent = None;
            n.prev_sibling = None;
            n.next_sibling = None;
        })?;

        Some(NodeKey(parent))
    }

    // All inputs are validated and `id` detached before linking a gap.
    fn attach(
        &mut self,
        parent: Entity,
        id: Entity,
        prev: Option<Entity>,
        next: Option<Entity>,
    ) -> Option<()> {
        self.edit(id, |n| {
            n.parent = Some(parent);
            n.prev_sibling = prev;
            n.next_sibling = next;
        })?;

        if let Some(prev) = prev {
            self.edit(prev, |n| n.next_sibling = Some(id))?;
        } else {
            self.edit(parent, |n| n.first_child = Some(id))?;
        }

        if let Some(next) = next {
            self.edit(next, |n| n.prev_sibling = Some(id))?;
        } else {
            self.edit(parent, |n| n.last_child = Some(id))?;
        }

        Some(())
    }

    /// Append a subtree; returns its previous parent.
    pub fn append(&mut self, parent: NodeKey, id: NodeKey) -> Option<NodeKey> {
        if !self.can_attach(parent.0, id.0) {
            return None;
        }

        let old = self.remove_parent(id);
        let prev = self.links(parent.0)?.last_child;
        self.attach(parent.0, id.0, prev, None)?;

        old
    }

    /// Prepend a subtree; returns its previous parent.
    pub fn prepend(&mut self, parent: NodeKey, id: NodeKey) -> Option<NodeKey> {
        if !self.can_attach(parent.0, id.0) {
            return None;
        }

        let old = self.remove_parent(id);
        let next = self.links(parent.0)?.first_child;
        self.attach(parent.0, id.0, None, next)?;

        old
    }

    /// Insert before an attached target; returns the previous parent.
    pub fn before(&mut self, target: NodeKey, id: NodeKey) -> Option<NodeKey> {
        let parent = self.links(target.0)?.parent?;

        if target == id {
            return Some(NodeKey(parent));
        }

        if !self.can_attach(parent, id.0) {
            return None;
        }

        let old = self.remove_parent(id);
        let prev = self.links(target.0)?.prev_sibling;
        self.attach(parent, id.0, prev, Some(target.0))?;

        old
    }

    /// Insert after an attached target; returns the previous parent.
    pub fn after(&mut self, target: NodeKey, id: NodeKey) -> Option<NodeKey> {
        let parent = self.links(target.0)?.parent?;

        if target == id {
            return Some(NodeKey(parent));
        }

        if !self.can_attach(parent, id.0) {
            return None;
        }

        let old = self.remove_parent(id);
        let next = self.links(target.0)?.next_sibling;
        self.attach(parent, id.0, Some(target.0), next)?;

        old
    }

    /// Delete a whole subtree without using the call stack for traversal.
    pub fn delete(&mut self, id: NodeKey) {
        if self.links(id.0).is_none() {
            return;
        }

        self.remove_parent(id);

        let mut pending = vec![id.0];
        while let Some(id) = pending.pop() {
            let mut child = self.links(id).and_then(|n| n.first_child);

            while let Some(next) = child {
                pending.push(next);
                child = self.links(next).and_then(|n| n.next_sibling);
            }

            let _ = self.world.despawn(id);
        }
    }

    /// Remove every element. Keys may be reused after clearing.
    pub fn clear(&mut self) {
        self.world.clear();
    }

    /// Iterate from `start` through its following siblings, inclusive.
    /// Reversing this iterator reverses that same range.
    pub fn cursor(&self, start: Option<NodeKey>) -> Cursor<'_> {
        Cursor::new(self, start)
    }

    pub fn children(&self, parent: NodeKey) -> Cursor<'_> {
        self.cursor(self.first_child(parent))
    }
}

pub struct Props<'a>(EntityRef<'a>);

impl<'a> Props<'a> {
    pub fn get<T: StyleProp>(&self) -> Option<Ref<'a, T>> {
        self.0.get::<&T>()
    }

    pub fn get_mut<T: StyleProp>(&self) -> Option<RefMut<'a, T>> {
        self.0.get::<&mut T>()
    }
}

/// Private structural component, distinct from the public node kind.
#[derive(Default, Clone, Copy)]
struct Relation {
    parent: Option<Entity>,

    first_child: Option<Entity>,
    last_child: Option<Entity>,

    prev_sibling: Option<Entity>,
    next_sibling: Option<Entity>,
}

/// Private wrapper for storing the required node kind in the world.
struct Kind(Node);
