//! Layout engine shared by the builder and the Taffy adapter.
//!
//! Every Taffy node is a retained fragment: the box of an item or an anonymous
//! block. Layout state and inline layouts are keyed by those fragments, and
//! Taffy traverses their fragment children.
//!
//! - [`block`] lays out boxes with Taffy, including custom containers.
//! - [`inline`] lays out inline content with Parley.
//! - [`retain`] keeps retained fragments and their state in sync.

mod block;
mod inline;
mod retain;
#[cfg(test)]
mod tests;

use comet_tree::ui::Ui;
use rustc_hash::FxHashSet;
use slotmap::{Key, KeyData};

use super::{
    FragmentItem, FragmentKey, StateKey,
    adapter::{Child, FragmentEngine},
    state::{Mode, NodeLayoutBits, NodeState},
    tree::FragmentTree,
};
use crate::{
    dirty_map::{DirtyFlags, DirtyMap},
    input::{ChildLayout, LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
    style::LayoutConstraint,
    taffy::style::{self, Style},
};

/// The Taffy node of a box or anonymous block, identified by its state.
pub(crate) fn node_id(state: StateKey) -> taffy::NodeId {
    taffy::NodeId::new(state.data().as_ffi())
}

/// The layout state of a Taffy node.
pub(crate) fn state_of(id: taffy::NodeId) -> StateKey {
    KeyData::from_ffi(id.into()).into()
}

/// Flags marking an item one of whose fragments a build created, moved or
/// resized: later steps resolve its paint data and its layers again.
pub(crate) const PAINT_CHANGED: DirtyFlags =
    DirtyFlags::NEED_PRE_PAINT.union(DirtyFlags::NEED_LAYER);

/// Whether the content of an item with `flags` changed: its child list, or,
/// with [`DirtyFlags::NEED_FULL_LAYOUT`], which includes
/// [`DirtyFlags::NEED_CHILDREN`], its kind, text style or content. Its box
/// finds its mode and attaches its children again, and inline content
/// including it is shaped again.
pub(in crate::fragment::engine) fn content_changed(flags: DirtyFlags) -> bool {
    flags.contains(DirtyFlags::NEED_CHILDREN)
}

/// Whether an item with `flags` needs layout. [`PAINT_CHANGED`] flags alone
/// are paint changes for later steps, so the item keeps its layout.
/// [`DirtyFlags::NEED_CHILDREN`], which includes
/// [`DirtyFlags::NEED_LAYER`], still does.
pub(in crate::fragment::engine) fn layout_changed(flags: DirtyFlags) -> bool {
    !flags.difference(PAINT_CHANGED).is_empty()
}

/// Buffers retained by the builder between builds.
#[derive(Default)]
pub(crate) struct Scratch {
    stack: Vec<LayoutInputKey>,
    /// Retained fragments detached from their parents during this build. Those
    /// left without a parent at the end are deleted.
    detached: Vec<FragmentKey>,
    /// Box fragments created during this build, whose mode and style are
    /// already resolved.
    created: FxHashSet<FragmentKey>,
    /// Stack of the direct children of fragments being laid out, shared by
    /// nested [`FragmentEngine`]s.
    pub(super) children: Vec<Child>,
    inline: inline::Scratch,
}

pub(crate) struct Engine<'a> {
    pub ui: &'a Ui,
    pub tree: &'a LayoutInputTree,
    /// Input changes to consume. Items whose fragments perform layout are
    /// marked [`DirtyFlags::NEED_PRE_PAINT`] in it.
    pub dirty: &'a mut DirtyMap,
    pub fragments: &'a mut FragmentTree,
    pub fonts: &'a mut parley::FontContext,
    pub parley: &'a mut parley::LayoutContext<Option<LayoutInputKey>>,
    pub constraint: &'a LayoutConstraint,
    pub root: LayoutInputKey,
    pub scratch: &'a mut Scratch,
}

impl Engine<'_> {
    /// Block level and atomic inline items, the root, and every child of a
    /// container generate a box.
    fn is_box(&self, key: LayoutInputKey) -> bool {
        key == self.root
            || matches!(
                self.tree.items[key].kind,
                LayoutInputKind::Block | LayoutInputKind::AtomicInline
            )
            || self.tree.items.parent(key).is_some_and(|parent| {
                self.tree.items[parent].child_layout == ChildLayout::Container
            })
    }

    /// How a box generating item lays out its content.
    fn mode(&self, key: LayoutInputKey) -> Mode {
        let item = &self.tree.items[key];

        let inline = match item.child_layout {
            ChildLayout::None => item.kind == LayoutInputKind::Text,
            ChildLayout::Inline => true,
            ChildLayout::Block => self
                .tree
                .items
                .children(key)
                .any(|child| self.tree.items[child].kind != LayoutInputKind::Block),
            ChildLayout::Container => return Mode::Container,
        };

        match (inline, item.child_layout) {
            (true, _) if self.has_block_content(key) => Mode::MixedInline,
            (true, _) => Mode::Inline,
            (false, ChildLayout::Block) => Mode::Block,
            _ => Mode::Leaf,
        }
    }

    /// The build constraint with the font size of an item, resolving `em`
    /// units of its box and of the anonymous blocks it owns.
    fn style_constraint(&self, key: LayoutInputKey) -> LayoutConstraint {
        let mut cx = self.constraint.clone();
        cx.units.font_size = self.tree.items[key].text_style.span.font_size as f64;

        cx
    }

    /// Resolve the box style of an item, with what its layout depends on.
    fn resolve_style(&self, key: LayoutInputKey) -> (Style, NodeLayoutBits) {
        let mut bits = NodeLayoutBits::empty();
        let style = style::resolve_tracked(
            self.ui.props(self.tree.items[key].node),
            &self.style_constraint(key),
            &mut |unit| bits |= NodeLayoutBits::of_unit(unit),
        );

        (style, bits)
    }

    /// Add the [propagated](NodeLayoutBits::PROPAGATED) part of `bits` to the
    /// box fragments of the ancestors of an item, up to the root of the
    /// build. Items without boxes, such as inline elements, are skipped.
    fn propagate_bits(&mut self, key: LayoutInputKey, bits: NodeLayoutBits) {
        let bits = bits & NodeLayoutBits::PROPAGATED;

        if bits.is_empty() || key == self.root {
            return;
        }

        let mut ancestor = self.tree.items.parent(key);

        while let Some(key) = ancestor {
            if let Some(state) = self
                .fragments
                .box_fragment(key)
                .and_then(|fragment| self.fragments.get_state_mut(fragment))
            {
                // Bits always reach the root, so ancestors above have them.
                if state.bits.contains(bits) {
                    break;
                }

                state.bits |= bits;
            }

            if key == self.root {
                break;
            }

            ancestor = self.tree.items.parent(key);
        }
    }

    /// The box fragment of a box generating item, created with its state if
    /// missing.
    pub fn ensure_box(&mut self, key: LayoutInputKey) -> FragmentKey {
        if let Some(fragment) = self.fragments.box_fragment(key) {
            return fragment;
        }

        let (style, bits) = self.resolve_style(key);
        let mut state = NodeState::new(self.mode(key), style);
        state.bits = bits;
        let fragment =
            self.fragments
                .insert_with_state(key, |state| FragmentItem::Box { state }, state);

        self.fragments.first.insert(key, fragment);
        self.scratch.created.insert(fragment);
        self.dirty.mark(self.tree, key, PAINT_CHANGED);
        self.propagate_bits(key, bits);

        fragment
    }

    /// Visit flagged items from the root. Cached layouts of flagged items are
    /// dropped and their fragment children are synchronized. Subtrees are
    /// visited only when their root's
    /// [`DirtyEntry::child_bits`](crate::dirty_map::DirtyEntry::child_bits)
    /// have [`DirtyFlags::NEED_BOX_LAYOUT`] or [`DirtyFlags::NEED_CHILDREN`].
    /// Flags are kept.
    pub fn invalidate(&mut self) {
        // Without retained fragments every item is treated as flagged.
        let force = self.fragments.box_fragment(self.root).is_none();
        self.scratch.created.clear();
        let mut stack = std::mem::take(&mut self.scratch.stack);
        stack.push(self.root);

        while let Some(key) = stack.pop() {
            let entry = self.dirty.entry(key);
            let flags = entry.flags;
            // `NEED_BOX_LAYOUT` is part of every flag laying out a box, and a
            // child list change alone carries `NEED_CHILDREN` only.
            let descend = entry.child_bits.contains(DirtyFlags::NEED_BOX_LAYOUT)
                || entry.child_bits.contains(DirtyFlags::NEED_CHILDREN);
            let child_layout = self.tree.items[key].child_layout;
            let is_box = self.is_box(key);

            // After a viewport change, boxes depending on it are laid out
            // again. Their ancestor boxes carry the bit too, which leads here.
            // Viewport dependent text styles are flagged by the input builder.
            let viewport = is_box
                && self.constraint.viewport_changed
                && self
                    .fragments
                    .box_fragment(key)
                    .and_then(|fragment| self.fragments.get_state(fragment))
                    .is_some_and(|state| state.bits.contains(NodeLayoutBits::DEPENDS_VIEWPORT));

            // Ancestors of flagged items are laid out again too.
            // Paint-only changes are left to later steps.
            if !force && !viewport && !layout_changed(flags) && !descend {
                continue;
            }

            if is_box {
                self.invalidate_box(key, flags, force, viewport);
            } else if let Some(fragment) = self.fragments.box_fragment(key) {
                // The item no longer generates a box.
                self.drop_state(fragment);
                self.release(fragment);
            }

            if (force || viewport || descend) && child_layout != ChildLayout::None {
                stack.extend(self.tree.items.children(key));
            }

            // Inline elements carry no bits and are only visited when flagged,
            // so boxes inside them are checked from their owner. Those
            // without the bit are skipped when visited.
            if viewport
                && self
                    .fragments
                    .box_fragment(key)
                    .and_then(|fragment| self.fragments.get_state(fragment))
                    .is_some_and(|state| matches!(state.mode, Mode::Inline | Mode::MixedInline))
            {
                self.push_nested_inline_boxes(key, &mut stack);
            }
        }

        self.scratch.stack = stack;
    }

    /// Refresh a flagged box and rebuild its fragment children. Lines are
    /// attached again when they are emitted.
    ///
    /// Boxes created in this build are already resolved. The style of a box
    /// is kept unless its box is dirty, and its mode unless its
    /// [content changed](content_changed): the mode depends on the kinds of
    /// its children and inline content.
    ///
    /// Box style changes alone keep shaped text and child fragments. Inline
    /// content is shaped again only when the content of an item in it changed.
    ///
    /// `viewport` lays out a box depending on a changed viewport again, as if
    /// its box style changed.
    fn invalidate_box(
        &mut self,
        key: LayoutInputKey,
        flags: DirtyFlags,
        force: bool,
        viewport: bool,
    ) {
        let fragment = self.ensure_box(key);
        let created = self.scratch.created.contains(&fragment);
        let box_dirty = force || viewport || flags.contains(DirtyFlags::NEED_BOX_LAYOUT);
        let dirty = force || content_changed(flags);
        // Finding the mode walks the children and inline content.
        let mode = (!created && dirty).then(|| self.mode(key));
        let style = (!created && box_dirty).then(|| self.resolve_style(key));
        let Some(state) = self.fragments.get_state_mut(fragment) else {
            return;
        };
        let previous = state.mode;
        let mut bits = NodeLayoutBits::empty();

        if let Some(mode) = mode {
            state.mode = mode;
        }

        if let Some((style, style_bits)) = style {
            state.style = style;
            state.bits |= style_bits;
            bits = style_bits;
        }

        let mode = state.mode;
        state.invalidate();
        self.propagate_bits(key, bits);

        match mode {
            Mode::Inline => {
                self.release_children(fragment);

                if (dirty || self.inline_content_dirty(key))
                    && let Some(inline) = self.fragments.get_inline_mut(fragment)
                {
                    inline.shaped = false;
                }
            }
            Mode::MixedInline => {
                self.fragments.take_inline(fragment);
                self.rebuild_runs(key, fragment, force);
            }
            Mode::Block | Mode::Container => {
                self.fragments.take_inline(fragment);

                // Child list changes flag the box itself, so its children are
                // still attached unless it is new or changed mode.
                if !created && !dirty && mode == previous {
                    return;
                }

                // Children whose boxes are already attached in order stay in
                // place. The rest are released from the first mismatch and
                // attached again.
                let tree = self.tree;
                let mut inputs = tree.items.children(key).peekable();
                let mut kept = None;

                while let Some(&child) = inputs.peek() {
                    let attached = match kept {
                        Some(kept) => self.fragments.items.next_sibling(kept),
                        None => self.fragments.items.first_child(fragment),
                    };

                    if attached.is_none() || self.fragments.box_fragment(child) != attached {
                        break;
                    }

                    kept = attached;
                    inputs.next();
                }

                self.release_after(fragment, kept);

                for child in inputs {
                    let child = self.ensure_box(child);
                    self.fragments.items.append(fragment, child);
                }
            }
            Mode::Leaf | Mode::Run { .. } => {
                self.fragments.take_inline(fragment);
                self.release_children(fragment);
            }
        }
    }

    /// Mark the final layout of a fragment and its laid out ancestors invalid.
    /// Measuring replaces retained lines and descendant geometry, so the final
    /// layout must run again even when its cache entry matches.
    fn invalidate_final(&mut self, fragment: FragmentKey) {
        let root = self.fragments.box_fragment(self.root);
        let mut ancestor = Some(fragment);

        while let Some(key) = ancestor {
            if let Some(state) = self.fragments.get_state_mut(key) {
                state.final_layout_valid = false;
            }

            if Some(key) == root {
                break;
            }

            ancestor = self.fragments.items.parent(key);
        }
    }

    /// Lay out a fragment with the algorithm of its mode.
    pub fn layout_child(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        let state = state_of(id);

        let Some((fragment, mode)) = self
            .fragments
            .states
            .get(state)
            .map(|state| (state.fragment, state.mode))
        else {
            return taffy::LayoutOutput::HIDDEN;
        };

        match mode {
            Mode::Inline | Mode::Run { .. } => self.layout_lines(fragment, input),
            Mode::Block => self.layout_block(fragment, state, input, block_ctx),
            Mode::MixedInline => {
                let output = self.layout_block(fragment, state, input, block_ctx);

                if input.run_mode == taffy::RunMode::PerformLayout {
                    self.link_inline(fragment);
                }

                output
            }
            Mode::Container => self.layout_container(fragment, state, input),
            Mode::Leaf => self.layout_leaf(fragment, input),
        }
    }
}
