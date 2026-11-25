//! Step 1 of a build: reflect child changes into the box structure.
//!
//! Only subtrees whose items are marked [`DirtyFlags::NEED_CHILDREN`], which
//! [`DirtyFlags::NEED_FULL_LAYOUT`] includes, are visited. Nothing is laid
//! out, and box keys are kept:
//! - A box whose content layout kind changed takes the new kind in place.
//! - A block or container box marked itself reconciles its child boxes.
//! - An inline formatting context whose inline content changed is marked
//!   to be built again when it is laid out.
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
        inline::input::{InlinePrelayout, InputItem},
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
            None => self.create(root, Some(BoxFragmentType::Normal)),
        };
        self.root_box = Some(key);

        self.run_tasks(0);
        self.propagate_created_bits(0);

        key
    }

    /// Run the pending tasks above the first `depth` ones.
    pub(super) fn run_tasks(&mut self, depth: usize) {
        while self.scratch.tasks.len() > depth {
            match self.scratch.tasks.pop() {
                Some(Task::Sync(item, key)) => self.sync(item, key),
                Some(Task::Children(item, key)) => self.append_children(item, key),
                None => return,
            }
        }
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
            self.change_kind(item, key, kind);
            return;
        }

        let entry = self.dirty.entry(item);
        match kind {
            ChildKind::Inline => {
                if self.needs_inline_build(item) {
                    self.mark_inline_unbuilt(key);
                }

                if entry.child_bits.contains(DirtyFlags::NEED_CHILDREN) {
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

    /// Mark the inline content of the box `key` to be built again when it is
    /// laid out, and drop the cached layouts of it and its ancestors.
    fn mark_inline_unbuilt(&mut self, key: BoxFragmentKey) {
        if let Some(inline) = self
            .fragments
            .get_mut(key)
            .and_then(|fragment| fragment.inline_mut())
        {
            inline.built = false;
        }

        self.invalidate_up(key);
    }

    /// Give the box `key` of `item` the content layout `kind` in place. Its
    /// child boxes are reconciled with its new content: right away for block
    /// and container layouts, when the inline content is built for inline
    /// layouts, and deleted for content.
    fn change_kind(&mut self, item: LayoutInputKey, key: BoxFragmentKey, kind: ChildKind) {
        self.fragments.boxes[key].child_layout = match kind {
            ChildKind::Block => ChildLayoutType::Block,
            ChildKind::Inline => ChildLayoutType::Inline(Box::default()),
            ChildKind::Container => ChildLayoutType::Container,
            ChildKind::Content => ChildLayoutType::Content,
        };

        match kind {
            ChildKind::Block | ChildKind::Container => self.reconcile(item, key),
            // Their own changes are synced now, as for the boxes of a built
            // inline layout.
            ChildKind::Inline => self.sync_child_boxes(key),
            ChildKind::Content => {
                while let Some(child) = self.fragments.boxes.first_child(key) {
                    self.fragments.delete(child);
                }
            }
        }

        self.invalidate_up(key);
        self.mark_paint(item);
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
    /// the children of `item`.
    fn reconcile(&mut self, item: LayoutInputKey, parent: BoxFragmentKey) {
        let tree = self.cx.tree;
        let changed = self.place_children(
            parent,
            tree.items.children(item).map(|child| (child, None)),
            true,
        );

        if changed {
            self.invalidate_up(parent);
        }
    }

    /// Make the child boxes of `parent` the boxes of `children`, in order.
    /// Returns whether any box was added, moved, retyped or deleted.
    ///
    /// Each child comes with its box type, or `None` for the type of a child
    /// of a block or container box. Boxes of remaining children are moved
    /// into place, keeping their keys, and with `sync`, visited for their own
    /// changes. Boxes are created for new children, with their children
    /// built by the tasks they push, and the rest are deleted. The placed
    /// children are left in the key buffer of the scratch.
    pub(super) fn place_children(
        &mut self,
        parent: BoxFragmentKey,
        children: impl Iterator<Item = (LayoutInputKey, Option<BoxFragmentType>)>,
        sync: bool,
    ) -> bool {
        let mut placed = std::mem::take(&mut self.scratch.keys);
        placed.clear();
        let mut changed = false;
        // The child box placed last.
        let mut cursor: Option<BoxFragmentKey> = None;

        for (child, box_type) in children {
            let existing = self
                .fragments
                .box_fragment(child)
                .filter(|&key| self.fragments.boxes.parent(key) == Some(parent));

            let key = match existing {
                Some(key) => {
                    let fragment = &mut self.fragments.boxes[key];
                    let box_type =
                        box_type.unwrap_or_else(|| classify::block_box_type(&fragment.style));

                    if fragment.box_type != box_type {
                        fragment.box_type = box_type;
                        fragment.invalidate();
                        changed = true;
                    }

                    if sync && self.needs_visit(child) {
                        self.scratch.tasks.push(Task::Sync(child, key));
                    }

                    key
                }
                None => {
                    changed = true;
                    self.create(child, box_type)
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

            placed.push((child, key));
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

        self.scratch.keys = placed;
        changed
    }

    /// Create the detached box of `item`. Its children are built by the
    /// tasks it pushes, and an inline formatting context when it is laid
    /// out.
    ///
    /// The box is a `box_type` box, or with `None`, the type of a child of a
    /// block or container box.
    fn create(
        &mut self,
        item: LayoutInputKey,
        box_type: Option<BoxFragmentType>,
    ) -> BoxFragmentKey {
        let kind = ChildKind::of_item(self.tree(), item);
        let (style, bits) = self.cx.resolve_style(item);
        let box_type = box_type.unwrap_or_else(|| classify::block_box_type(&style));
        let layout = match kind {
            ChildKind::Block => ChildLayoutType::Block,
            ChildKind::Inline => ChildLayoutType::Inline(Box::default()),
            ChildKind::Container => ChildLayoutType::Container,
            ChildKind::Content => ChildLayoutType::Content,
        };

        let mut fragment = BoxFragment::new(item, box_type, layout, style);
        fragment.bits = bits;
        let key = self.fragments.insert(fragment);

        if matches!(kind, ChildKind::Block | ChildKind::Container) {
            self.scratch.tasks.push(Task::Children(item, key));
        }

        self.scratch.created.insert(key, ());
        self.scratch.created_order.push(key);
        self.mark_paint(item);

        key
    }

    /// Append new boxes for the children of `item` to its new block or
    /// container box `parent`.
    fn append_children(&mut self, item: LayoutInputKey, parent: BoxFragmentKey) {
        let tree = self.cx.tree;

        for child in tree.items.children(item) {
            let key = self.create(child, None);
            self.fragments.boxes.append(parent, key);
        }
    }

    /// Add the [propagated](crate::fragment::state::NodeLayoutBits::PROPAGATED)
    /// bits of the boxes created by this build, from the `start`th one, to
    /// their ancestors.
    pub(super) fn propagate_created_bits(&mut self, start: usize) {
        for index in start..self.scratch.created_order.len() {
            let key = self.scratch.created_order[index];

            if let Some(bits) = self.fragments.get(key).map(|fragment| fragment.bits) {
                self.propagate_bits(key, bits);
            }
        }
    }
}
