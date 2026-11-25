//! Step 1 of a build: reflect child changes into the box structure.
//!
//! Only subtrees whose items are marked [`DirtyFlags::NEED_CHILDREN`], which
//! [`DirtyFlags::NEED_FULL_LAYOUT`] includes, are visited. Nothing is laid
//! out:
//! - A box whose content layout kind changed is built again.
//! - A block or container box marked itself reconciles its child boxes,
//!   keeping the keys of the boxes of remaining children.
//! - An inline formatting context whose inline content changed is built
//!   again, reusing the boxes inside it.
//!
//! Boxes are only ever moved within their parent, since items never move
//! across parents.

use super::{
    Engine, Task,
    classify::{self, ChildKind},
};
use crate::{
    dirty_map::DirtyFlags,
    fragment2::{
        BoxFragment, BoxFragmentKey, BoxFragmentType, ChildLayoutType,
        inline::{
            builder::InlineFragmentainer,
            input::{InlinePrelayout, InputItem},
        },
    },
    input::LayoutInputKey,
};

impl Engine<'_> {
    /// Sync the boxes of the build root with the marked child changes,
    /// returning the box of the root.
    pub(super) fn sync_children(&mut self) -> BoxFragmentKey {
        let root = self.root;
        let key = match self.fragments.box_fragment(root) {
            Some(key) => {
                self.scratch.tasks.push(Task::Sync(root, key));
                key
            }
            None => self.create(root, Some(BoxFragmentType::Normal), None),
        };
        self.root_box = Some(key);

        while let Some(task) = self.scratch.tasks.pop() {
            match task {
                Task::Sync(item, key) => self.sync(item, key),
                Task::Children(item, key) => self.append_children(item, key),
                Task::Inline(fragmentainer) => self.drain(fragmentainer),
            }
        }

        self.propagate_created_bits();

        self.root_box.unwrap_or(key)
    }

    /// Whether the item or its subtree has child changes.
    fn needs_visit(&self, item: LayoutInputKey) -> bool {
        let entry = self.dirty.entry(item);

        entry.flags.contains(DirtyFlags::NEED_CHILDREN)
            || entry.child_bits.contains(DirtyFlags::NEED_CHILDREN)
    }

    /// Sync the attached box `key` of `item`.
    fn sync(&mut self, item: LayoutInputKey, key: BoxFragmentKey) {
        if self.tree().items.get(item).is_none()
            || self
                .fragments
                .get(key)
                .is_none_or(|fragment| fragment.source != item)
            || !self.needs_visit(item)
        {
            return;
        }

        let kind = ChildKind::of_item(self.tree(), item);
        if ChildKind::of(&self.fragments.boxes[key].child_layout) != kind {
            self.replace(item, key);
            return;
        }

        let entry = self.dirty.entry(item);
        match kind {
            ChildKind::Inline => {
                if self.needs_inline_build(item) {
                    self.replace(item, key);
                } else if entry.child_bits.contains(DirtyFlags::NEED_CHILDREN) {
                    self.sync_atomics(key);
                }
            }
            ChildKind::Block | ChildKind::Container => {
                if entry.flags.contains(DirtyFlags::NEED_CHILDREN) {
                    self.reconcile(item, key);
                } else if entry.child_bits.contains(DirtyFlags::NEED_CHILDREN) {
                    self.sync_child_boxes(key);
                }
            }
            ChildKind::Content => {}
        }
    }

    /// Whether the inline formatting context of `item` needs to be built
    /// again: the item, an inline element or text inside its inline content
    /// had child changes, or a box inside it was marked
    /// [`DirtyFlags::NEED_FULL_LAYOUT`].
    ///
    /// A box moving in or out of flow changes the kind of its Parley inline
    /// box, which can only be set by building the inline layout again. The
    /// caller marks such a change [`DirtyFlags::NEED_FULL_LAYOUT`], and other
    /// position changes [`DirtyFlags::NEED_BOX_LAYOUT`]. Other child changes
    /// of a box inside are left to that box.
    fn needs_inline_build(&self, item: LayoutInputKey) -> bool {
        let entry = self.dirty.entry(item);

        if entry.flags.contains(DirtyFlags::NEED_CHILDREN) {
            return true;
        }

        if !entry.child_bits.contains(DirtyFlags::NEED_CHILDREN) {
            return false;
        }

        InlinePrelayout::new(self.tree(), item).any(|input| {
            let (content, flags) = match input {
                InputItem::Push(content) | InputItem::Text(content) => {
                    (content, DirtyFlags::NEED_CHILDREN)
                }
                InputItem::Atomic(child) | InputItem::Block(child) => {
                    (child, DirtyFlags::NEED_FULL_LAYOUT)
                }
                InputItem::Pop => return false,
            };

            self.dirty.entry(content).flags.contains(flags)
        })
    }

    /// Visit the boxes inside the inline content of `owner` with child
    /// changes.
    fn sync_atomics(&mut self, owner: BoxFragmentKey) {
        let mut keys = std::mem::take(&mut self.scratch.keys);
        keys.clear();

        if let Some(inline) = self.fragments.inline_layout(owner) {
            keys.extend(
                inline
                    .atomics
                    .iter()
                    .map(|(&item, &key)| (item, key))
                    .filter(|&(item, _)| self.needs_visit(item)),
            );
        }

        self.scratch
            .tasks
            .extend(keys.drain(..).map(|(item, key)| Task::Sync(item, key)));
        self.scratch.keys = keys;
    }

    /// Visit the child boxes of `parent` with child changes.
    fn sync_child_boxes(&mut self, parent: BoxFragmentKey) {
        let mut keys = std::mem::take(&mut self.scratch.keys);
        keys.clear();
        keys.extend(
            self.fragments
                .boxes
                .children(parent)
                .map(|key| (self.fragments.boxes[key].source, key))
                .filter(|&(item, _)| self.needs_visit(item)),
        );

        self.scratch
            .tasks
            .extend(keys.drain(..).map(|(item, key)| Task::Sync(item, key)));
        self.scratch.keys = keys;
    }

    /// Match the child boxes of the block or container box `parent` with
    /// the children of `item`. Boxes of remaining children are moved into
    /// place, boxes are created for new children, and the rest are deleted.
    fn reconcile(&mut self, item: LayoutInputKey, parent: BoxFragmentKey) {
        let tree = self.cx.tree;
        let mut changed = false;
        // The child box placed last.
        let mut cursor: Option<BoxFragmentKey> = None;

        for child in tree.items.children(item) {
            let existing = self
                .fragments
                .box_fragment(child)
                .filter(|&key| self.fragments.boxes.parent(key) == Some(parent));

            let key = match existing {
                Some(key) => {
                    if self.needs_visit(child) {
                        self.scratch.tasks.push(Task::Sync(child, key));
                    }

                    key
                }
                None => {
                    changed = true;
                    self.create(child, None, None)
                }
            };

            let expected = match cursor {
                Some(cursor) => self.fragments.boxes.next_sibling(cursor),
                None => self.fragments.boxes.first_child(parent),
            };

            if expected != Some(key) {
                match cursor {
                    Some(cursor) => self.fragments.boxes.after(cursor, key),
                    None => self.fragments.boxes.prepend(parent, key),
                };

                if existing.is_some() {
                    changed = true;
                    self.mark_paint(child);
                }
            }

            cursor = Some(key);
        }

        let mut stale = match cursor {
            Some(cursor) => self.fragments.boxes.next_sibling(cursor),
            None => self.fragments.boxes.first_child(parent),
        };

        while let Some(key) = stale {
            stale = self.fragments.boxes.next_sibling(key);
            self.fragments.delete(key);
            changed = true;
        }

        if changed {
            self.invalidate_up(parent);
        }
    }

    /// Build the box of `item` again in place of its box `old`.
    fn replace(&mut self, item: LayoutInputKey, old: BoxFragmentKey) {
        let parent = self.fragments.boxes.parent(old);
        let prev = self.fragments.boxes.prev_sibling(old);
        let inline_parent =
            parent.is_some_and(|parent| self.fragments.boxes[parent].child_layout.is_inline());

        // Inside inline content, the box type is decided by the inline
        // build of the parent, which is not built again.
        let box_type = if inline_parent || item == self.root {
            Some(self.fragments.boxes[old].box_type)
        } else {
            None
        };

        self.fragments.boxes.remove_parent(old);
        let key = self.create(item, box_type, Some(old));

        match (prev, parent) {
            (Some(prev), _) => {
                self.fragments.boxes.after(prev, key);
            }
            (None, Some(parent)) => {
                self.fragments.boxes.prepend(parent, key);
            }
            (None, None) => {}
        }

        if inline_parent
            && let Some(inline) = parent
                .and_then(|parent| self.fragments.get_mut(parent))
                .and_then(|parent| parent.inline_mut())
        {
            inline.atomics.insert(item, key);
        }

        if item == self.root {
            self.root_box = Some(key);
        }

        self.invalidate_up(key);
    }

    /// Create the detached box of `item`, deleting `old`, a box built for it
    /// before. Its content is built by the tasks it pushes.
    ///
    /// The box is a `box_type` box, or with `None`, the type of a child of a
    /// block or container box. An inline formatting context reuses the
    /// boxes inside the inline content of `old`.
    fn create(
        &mut self,
        item: LayoutInputKey,
        box_type: Option<BoxFragmentType>,
        old: Option<BoxFragmentKey>,
    ) -> BoxFragmentKey {
        let kind = ChildKind::of_item(self.tree(), item);

        let key = match kind {
            ChildKind::Inline => {
                let (key, fragmentainer) = self.inline.build(
                    &mut self.cx,
                    self.fragments,
                    item,
                    box_type.unwrap_or(BoxFragmentType::Normal),
                    old,
                );

                if box_type.is_none() {
                    let fragment = &mut self.fragments.boxes[key];
                    fragment.box_type = classify::block_box_type(&fragment.style);
                }

                self.scratch.tasks.push(Task::Inline(fragmentainer));
                key
            }
            ChildKind::Block | ChildKind::Container | ChildKind::Content => {
                if let Some(old) = old {
                    self.fragments.delete(old);
                }

                let (style, bits) = self.cx.resolve_style(item);
                let box_type = box_type.unwrap_or_else(|| classify::block_box_type(&style));
                let layout = match kind {
                    ChildKind::Block => ChildLayoutType::Block,
                    ChildKind::Container => ChildLayoutType::Container,
                    _ => ChildLayoutType::Content,
                };

                let mut fragment = BoxFragment::new(item, box_type, layout, style);
                fragment.bits = bits;
                let key = self.fragments.insert(fragment);

                if kind != ChildKind::Content {
                    self.scratch.tasks.push(Task::Children(item, key));
                }

                key
            }
        };

        self.scratch.created.insert(key, ());
        self.mark_paint(item);

        key
    }

    /// Append new boxes for the children of `item` to its new block or
    /// container box `parent`.
    fn append_children(&mut self, item: LayoutInputKey, parent: BoxFragmentKey) {
        let tree = self.cx.tree;

        for child in tree.items.children(item) {
            let key = self.create(child, None, None);
            self.fragments.boxes.append(parent, key);
        }
    }

    /// Build and attach the boxes inside the inline content of a box built
    /// by the inline layout builder. Old boxes of the same content layout
    /// kind are attached again, and visited for their own changes.
    fn drain(&mut self, mut fragmentainer: InlineFragmentainer) {
        let tree = self.cx.tree;

        while let Some(child) = fragmentainer.next(self.fragments) {
            let kind = ChildKind::of_item(tree, child.item);
            let reusable = child.old.filter(|&old| {
                self.fragments
                    .get(old)
                    .is_some_and(|fragment| ChildKind::of(&fragment.child_layout) == kind)
            });

            let key = match reusable {
                Some(old) => {
                    let fragment = &mut self.fragments.boxes[old];
                    if fragment.box_type != child.box_type {
                        fragment.box_type = child.box_type;
                        fragment.invalidate();
                    }

                    if self.needs_visit(child.item) {
                        self.scratch.tasks.push(Task::Sync(child.item, old));
                    }

                    old
                }
                None => self.create(child.item, Some(child.box_type), child.old),
            };

            child.attach(self.fragments, key);
        }
    }

    /// Add the [propagated](NodeLayoutBits::PROPAGATED) bits of the created
    /// boxes to their ancestors.
    fn propagate_created_bits(&mut self) {
        let mut keys = std::mem::take(&mut self.scratch.boxes);
        keys.clear();
        keys.extend(self.scratch.created.keys());

        for &key in &keys {
            if let Some(bits) = self.fragments.get(key).map(|fragment| fragment.bits) {
                self.propagate_bits(key, bits);
            }
        }

        self.scratch.boxes = keys;
    }
}
