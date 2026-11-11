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

use comet_tree::ui::Ui;
use rustc_hash::FxHashSet;
use slotmap::{Key, KeyData};

use super::{
    Fragment, FragmentBox, FragmentItem, FragmentKey,
    adapter::FragmentEngine,
    state::{Mode, NodeState},
    tree::FragmentTree,
};
use crate::{
    input::{ChildLayout, LayoutFlags, LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
    style::LayoutConstraint,
    taffy::style::{self, Style},
};

/// The Taffy node of a retained fragment.
pub(crate) fn node_id(fragment: FragmentKey) -> taffy::NodeId {
    taffy::NodeId::new(fragment.data().as_ffi())
}

/// The retained fragment of a Taffy node.
pub(crate) fn fragment_of(id: taffy::NodeId) -> FragmentKey {
    KeyData::from_ffi(id.into()).into()
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
    pub(super) children: Vec<FragmentKey>,
    inline: inline::Scratch,
}

pub(crate) struct Engine<'a> {
    pub ui: &'a Ui,
    pub tree: &'a LayoutInputTree,
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

    fn resolve_style(&self, key: LayoutInputKey) -> Style {
        style::resolve(
            self.ui,
            Some(self.tree.items[key].node),
            &self.style_constraint(key),
        )
    }

    /// The box fragment of a box generating item, created with its state if
    /// missing.
    pub fn ensure_box(&mut self, key: LayoutInputKey) -> FragmentKey {
        if let Some(fragment) = self.fragments.box_fragment(key) {
            return fragment;
        }

        let state = NodeState::new(self.mode(key), self.resolve_style(key));
        let fragment = self.fragments.items.insert(Fragment::new(
            key,
            FragmentItem::Box,
            FragmentBox::default(),
        ));

        self.fragments.states.insert(fragment, state);
        self.fragments.first.insert(key, fragment);
        self.scratch.created.insert(fragment);

        fragment
    }

    /// Visit flagged items from the root. Cached layouts of flagged items are
    /// dropped and their fragment children are synchronized. Flags are kept.
    pub fn invalidate(&mut self) {
        // Without retained fragments every item is treated as flagged.
        let force = self.fragments.box_fragment(self.root).is_none();
        self.scratch.created.clear();
        let mut stack = std::mem::take(&mut self.scratch.stack);
        stack.push(self.root);

        while let Some(key) = stack.pop() {
            let item = &self.tree.items[key];
            let flags = item.flags;
            let child_layout = item.child_layout;

            if !force && flags.is_empty() {
                continue;
            }

            if self.is_box(key) {
                self.invalidate_box(key, flags, force);
            } else if let Some(fragment) = self.fragments.box_fragment(key) {
                // The item no longer generates a box.
                self.drop_state(fragment);
                self.release(fragment);
            }

            if (force || flags.contains(LayoutFlags::CHILDREN_DIRTY))
                && child_layout != ChildLayout::None
            {
                stack.extend(self.tree.items.children(key));
            }
        }

        self.scratch.stack = stack;
    }

    /// Refresh a flagged box and rebuild its fragment children. Lines are
    /// attached again when they are emitted.
    ///
    /// Boxes created in this build are already resolved. The style of a box
    /// is kept when only its descendants are flagged, but its mode is not:
    /// children changing kind only flag their parent's descendants.
    fn invalidate_box(&mut self, key: LayoutInputKey, flags: LayoutFlags, force: bool) {
        let fragment = self.ensure_box(key);
        let created = self.scratch.created.contains(&fragment);
        let dirty = force || flags.contains(LayoutFlags::DIRTY);
        let previous = self.fragments.states[fragment].mode;

        if !created {
            let mode = self.mode(key);
            let style = dirty.then(|| self.resolve_style(key));

            let state = &mut self.fragments.states[fragment];
            state.mode = mode;

            if let Some(style) = style {
                state.style = style;
            }
        }

        let state = &mut self.fragments.states[fragment];
        let mode = state.mode;
        state.invalidate();

        match mode {
            Mode::Inline => {
                self.release_children(fragment);

                if let Some(inline) = self.fragments.inline_layouts.get_mut(fragment) {
                    inline.shaped = false;
                }
            }
            Mode::MixedInline => {
                self.fragments.inline_layouts.remove(fragment);
                self.rebuild_runs(key, fragment, force);
            }
            Mode::Block | Mode::Container => {
                self.fragments.inline_layouts.remove(fragment);

                // Child list changes flag the box itself, so its children are
                // still attached unless it is new or changed mode.
                if !created && !dirty && mode == previous {
                    return;
                }

                self.release_children(fragment);

                for child in self.tree.items.children(key) {
                    let child = self.ensure_box(child);
                    self.fragments.items.append(fragment, child);
                }
            }
            Mode::Leaf | Mode::Run { .. } => {
                self.fragments.inline_layouts.remove(fragment);
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
            if let Some(state) = self.fragments.states.get_mut(key) {
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
        let fragment = fragment_of(id);

        match self.fragments.states[fragment].mode {
            Mode::Inline | Mode::Run { .. } => self.layout_lines(fragment, input),
            Mode::Block => self.layout_block(fragment, input, block_ctx),
            Mode::MixedInline => {
                let output = self.layout_block(fragment, input, block_ctx);

                if input.run_mode == taffy::RunMode::PerformLayout {
                    self.link_inline(fragment);
                }

                output
            }
            Mode::Container => self.layout_container(fragment, input),
            Mode::Leaf => self.layout_leaf(fragment, input),
        }
    }
}
