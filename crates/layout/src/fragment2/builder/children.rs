//! Step 1 of a build: reflect child changes into the box structure.
//!
//! Only subtrees whose items are marked [`DirtyFlags::NEED_CHILDREN`], which
//! [`DirtyFlags::NEED_FULL_LAYOUT`] includes, are visited. Nothing is laid
//! out, and box keys are kept:
//! - A box whose content layout kind changed takes the new kind in place.
//! - A block or container box marked itself reconciles its child boxes.
//! - An inline formatting context whose inline content changed is marked
//!   to be built again when it is laid out.
//! - A mixed inline box whose inline content changed splits it into runs
//!   again, reusing its run boxes, which are built again when laid out.
//!
//! Items never move across parents, so boxes only move within their parent,
//! or between the runs of a mixed inline box. Boxes that may be claimed again
//! under another parent are parked: detached, until a later reconcile in the
//! same build attaches them, or deleted at the end of the build.

use super::{
    Engine, Task,
    classify::{self, ChildKind},
};
use crate::{
    dirty_map::DirtyFlags,
    fragment2::{
        BoxFragment, BoxFragmentKey, BoxFragmentLink, BoxFragmentType, ChildLayoutType,
        InlineLayout,
        inline::{
            collect::{FlowItem, split_runs},
            input::{InlinePrelayout, InputItem},
        },
    },
    input::LayoutInputKey,
    taffy::style,
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

        let kind = ChildKind::of_item(&self.cx, item);
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
            ChildKind::MixedInline => {
                if self.needs_inline_build(item) {
                    self.reconcile_mixed(item, key);
                } else if entry.child_bits.contains(DirtyFlags::NEED_CHILDREN) {
                    self.sync_mixed_children(key);
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
    /// child boxes are parked, and reconciled with its new content: right
    /// away for block, container and mixed inline layouts, when the inline
    /// content is built for inline layouts. Those not claimed again are
    /// deleted at the end of the build.
    fn change_kind(&mut self, item: LayoutInputKey, key: BoxFragmentKey, kind: ChildKind) {
        self.park_children(key);
        self.fragments.boxes[key].child_layout = kind.layout();

        match kind {
            ChildKind::Block | ChildKind::Container => self.reconcile(item, key),
            ChildKind::MixedInline => self.reconcile_mixed(item, key),
            ChildKind::Inline | ChildKind::Content => {}
        }

        self.invalidate_up(key);
        self.mark_paint(item);
    }

    /// Detach the box `key`, keeping it for a later reconcile in this build.
    fn park(&mut self, key: BoxFragmentKey) {
        self.fragments.boxes.remove_parent(key);
        self.scratch.parked.push(key);
    }

    /// Park the child boxes of `parent`. Anonymous run boxes are deleted, and
    /// their own children parked.
    fn park_children(&mut self, parent: BoxFragmentKey) {
        while let Some(child) = self.fragments.boxes.first_child(parent) {
            if self.fragments.is_anonymous(child) {
                self.park_children(child);
                self.fragments.delete(child);
            } else {
                self.park(child);
            }
        }
    }

    /// Delete the parked boxes not attached again, at the end of a build.
    pub(super) fn delete_parked(&mut self) {
        for key in std::mem::take(&mut self.scratch.parked) {
            if self.fragments.get(key).is_some()
                && self.fragments.boxes.parent(key).is_none()
                && Some(key) != self.root_box
            {
                self.fragments.delete(key);
            }
        }
    }

    /// The box of `item` that may become a child of `parent`: one of its
    /// children, a parked box, or a child of another run of the same mixed
    /// inline box.
    fn reusable_box(&self, item: LayoutInputKey, parent: BoxFragmentKey) -> Option<BoxFragmentKey> {
        let key = self.fragments.box_fragment(item)?;
        let boxes = &self.fragments.boxes;

        match boxes.parent(key) {
            Some(current) if current == parent => Some(key),
            None if Some(key) != self.root_box => Some(key),
            Some(current)
                if self.fragments.is_anonymous(current)
                    && self.fragments.is_anonymous(parent)
                    && boxes.parent(current) == boxes.parent(parent) =>
            {
                Some(key)
            }
            _ => None,
        }
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
                .filter(|&key| !self.fragments.is_anonymous(key))
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

    /// Visit the block level boxes of the mixed inline box `owner`, and the
    /// boxes inside its runs, with child changes.
    fn sync_mixed_children(&mut self, owner: BoxFragmentKey) {
        self.sync_child_boxes(owner);

        let mut run = self.first_run(owner);
        while let Some(key) = run {
            self.sync_atomics(key);
            run = self.fragments.boxes[key].link.next;
        }
    }

    /// The first run box of the mixed inline box `owner`. The others follow
    /// by their links.
    pub(super) fn first_run(&self, owner: BoxFragmentKey) -> Option<BoxFragmentKey> {
        self.fragments.boxes.get(owner)?.child_layout.first_run()
    }

    /// Split the inline content of `item` into runs and block level boxes
    /// again, and make them the children of its mixed inline box `owner`.
    /// Run boxes are reused by the block they follow, and built again when
    /// laid out. The runs are linked in order.
    fn reconcile_mixed(&mut self, item: LayoutInputKey, owner: BoxFragmentKey) {
        let mut flow = std::mem::take(&mut self.scratch.flow);
        split_runs(&self.cx, item, &mut flow);

        let mut runs = std::mem::take(&mut self.scratch.runs);
        runs.clear();
        runs.extend(
            self.fragments
                .boxes
                .children(owner)
                .filter_map(|child| Some((self.fragments.inline_layout(child)?.run_start, child)))
                .filter(|&(_, child)| self.fragments.is_anonymous(child)),
        );

        let mut changed = false;
        let mut cursor: Option<BoxFragmentKey> = None;

        for &flow_item in &flow {
            let (key, existing) = match flow_item {
                FlowItem::Run(start) => match runs.iter().position(|&(run, _)| run == start) {
                    Some(index) => {
                        let key = runs.swap_remove(index).1;
                        self.mark_inline_unbuilt(key);
                        (key, true)
                    }
                    None => (self.create_run(item, start), false),
                },
                FlowItem::Block(block) => match self.reusable_box(block, owner) {
                    Some(key) => {
                        let parked = self.fragments.boxes.parent(key).is_none();
                        let fragment = &mut self.fragments.boxes[key];
                        let box_type = classify::block_box_type(&fragment.style);

                        if fragment.box_type != box_type {
                            fragment.box_type = box_type;
                            fragment.invalidate();
                            changed = true;
                        }

                        if parked || self.needs_visit(block) {
                            self.scratch.tasks.push(Task::Sync(block, key));
                        }

                        (key, true)
                    }
                    None => (self.create(block, None), false),
                },
            };

            changed |= !existing | self.place_after(owner, cursor, key, existing);
            cursor = Some(key);
        }

        // Runs and blocks no longer in the flow. Blocks may be claimed as
        // atomic inlines by the runs.
        let mut stale = match cursor {
            Some(cursor) => self.fragments.boxes.next_sibling(cursor),
            None => self.fragments.boxes.first_child(owner),
        };
        while let Some(key) = stale {
            stale = self.fragments.boxes.next_sibling(key);
            changed = true;

            if self.fragments.is_anonymous(key) {
                self.park_children(key);
                self.fragments.delete(key);
            } else {
                self.park(key);
            }
        }

        self.link_runs(owner);
        self.scratch.flow = flow;
        self.scratch.runs = runs;

        if changed {
            self.invalidate_up(owner);
        }
    }

    /// Create the anonymous box of the run of the inline content of `owner`
    /// after the block level box `start`, unbuilt. It has the anonymous
    /// style: the box style of the content is the owner's.
    fn create_run(
        &mut self,
        owner: LayoutInputKey,
        start: Option<LayoutInputKey>,
    ) -> BoxFragmentKey {
        let style = style::anonymous(self.cx.constraint);
        let layout = InlineLayout {
            run_start: start,
            ..Default::default()
        };
        let fragment = BoxFragment::new(
            owner,
            BoxFragmentType::Normal,
            ChildLayoutType::Inline(Box::new(layout)),
            style,
        );
        let key = self.fragments.insert_anonymous(fragment);

        self.scratch.created.insert(key, ());
        self.scratch.created_order.push(key);
        self.mark_paint(owner);

        key
    }

    /// Link the run boxes of the mixed inline box `owner` in order, marking
    /// those whose links changed, and record the first one in the owner.
    fn link_runs(&mut self, owner: BoxFragmentKey) {
        let mut prev: Option<BoxFragmentKey> = None;
        let mut first: Option<BoxFragmentKey> = None;
        let mut child = self.fragments.boxes.first_child(owner);

        while let Some(key) = child {
            child = self.fragments.boxes.next_sibling(key);

            if !self.fragments.is_anonymous(key) {
                continue;
            }

            first.get_or_insert(key);

            let link = BoxFragmentLink {
                prev,
                next: self.fragments.boxes[key].link.next,
            };
            self.set_link(key, link);

            if let Some(prev) = prev {
                let link = BoxFragmentLink {
                    next: Some(key),
                    ..self.fragments.boxes[prev].link
                };
                self.set_link(prev, link);
            }

            prev = Some(key);
        }

        if let Some(last) = prev {
            let link = BoxFragmentLink {
                next: None,
                ..self.fragments.boxes[last].link
            };
            self.set_link(last, link);
        }

        self.fragments.boxes[owner].child_layout = ChildLayoutType::MixedInline(first);
    }

    /// Set the link of the box `key`, marking its source when it changed:
    /// its split edges are painted differently.
    fn set_link(&mut self, key: BoxFragmentKey, link: BoxFragmentLink) {
        let fragment = &mut self.fragments.boxes[key];

        if fragment.link != link {
            fragment.link = link;
            let source = fragment.source;
            self.mark_paint(source);
        }
    }

    /// Move the box `key` right after `cursor` among the children of
    /// `parent`, or first. Returns whether a box already there moved.
    fn place_after(
        &mut self,
        parent: BoxFragmentKey,
        cursor: Option<BoxFragmentKey>,
        key: BoxFragmentKey,
        existing: bool,
    ) -> bool {
        let expected = match cursor {
            Some(cursor) => self.fragments.boxes.next_sibling(cursor),
            None => self.fragments.boxes.first_child(parent),
        };

        if expected == Some(key) {
            return false;
        }

        match cursor {
            Some(cursor) => self.fragments.boxes.after(cursor, key),
            None => self.fragments.boxes.prepend(parent, key),
        };

        if existing {
            let source = self.fragments.boxes[key].source;
            self.mark_paint(source);
        }

        existing
    }

    /// Make the child boxes of `parent` the boxes of `children`, in order.
    /// Returns whether any box was added, moved, retyped or deleted.
    ///
    /// Each child comes with its box type, or `None` for the type of a child
    /// of a block or container box. Boxes of remaining children are moved
    /// into place, keeping their keys, and with `sync`, visited for their own
    /// changes; boxes taken from elsewhere always are. Boxes are created for
    /// new children, with their children built by the tasks they push. The
    /// rest are deleted, or parked for the other runs when `parent` is a run.
    /// The placed children are left in the key buffer of the scratch.
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
            let existing = self.reusable_box(child, parent);

            let key = match existing {
                Some(key) => {
                    let moved = self.fragments.boxes.parent(key) != Some(parent);
                    let fragment = &mut self.fragments.boxes[key];
                    let box_type =
                        box_type.unwrap_or_else(|| classify::block_box_type(&fragment.style));

                    if fragment.box_type != box_type {
                        fragment.box_type = box_type;
                        fragment.invalidate();
                        changed = true;
                    }

                    // Boxes coming from elsewhere were not visited with the
                    // children of this box.
                    if (sync || moved) && self.needs_visit(child) {
                        self.scratch.tasks.push(Task::Sync(child, key));
                    }

                    key
                }
                None => {
                    changed = true;
                    self.create(child, box_type)
                }
            };

            changed |= self.place_after(parent, cursor, key, existing.is_some());
            placed.push((child, key));
            cursor = Some(key);
        }

        // The boxes of a run may move to another run of the same box.
        let park = self.fragments.is_anonymous(parent);
        let mut stale = match cursor {
            Some(cursor) => self.fragments.boxes.next_sibling(cursor),
            None => self.fragments.boxes.first_child(parent),
        };

        while let Some(key) = stale {
            stale = self.fragments.boxes.next_sibling(key);
            changed = true;

            if park {
                self.park(key);
            } else {
                self.fragments.delete(key);
            }
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
        let kind = ChildKind::of_item(&self.cx, item);
        let (style, bits) = self.cx.resolve_style(item);
        let box_type = box_type.unwrap_or_else(|| classify::block_box_type(&style));

        let mut fragment = BoxFragment::new(item, box_type, kind.layout(), style);
        fragment.bits = bits;
        let key = self.fragments.insert(fragment);

        if matches!(
            kind,
            ChildKind::Block | ChildKind::Container | ChildKind::MixedInline
        ) {
            self.scratch.tasks.push(Task::Children(item, key));
        }

        self.scratch.created.insert(key, ());
        self.scratch.created_order.push(key);
        self.mark_paint(item);

        key
    }

    /// Append new boxes for the children of `item` to its new block,
    /// container or mixed inline box `parent`.
    fn append_children(&mut self, item: LayoutInputKey, parent: BoxFragmentKey) {
        if self.fragments.boxes[parent].child_layout.is_mixed_inline() {
            self.reconcile_mixed(item, parent);
            return;
        }

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
