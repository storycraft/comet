//! Layout engine shared by the builder and the Taffy adapter.
//!
//! - [`block`] lays out boxes with Taffy, including custom containers.
//! - [`inline`] lays out inline formatting contexts with Parley.
//! - [`retain`] keeps retained box fragments and their state in sync.

mod block;
mod inline;
mod retain;

use comet_tree::{node::NodeKey, ui::Ui};
use slotmap::{Key, KeyData};

use super::{
    Fragment, FragmentBox, FragmentItem, FragmentKey, state::NodeState, tree::FragmentTree,
};
use crate::{
    input::{ChildLayout, LayoutFlags, LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
    style::LayoutConstraint,
    taffy::style::{self, Style},
};

pub(crate) fn to_taffy_key(key: LayoutInputKey) -> taffy::NodeId {
    taffy::NodeId::new(key.data().as_ffi())
}

pub(super) fn from_taffy_key(id: taffy::NodeId) -> LayoutInputKey {
    KeyData::from_ffi(id.into()).into()
}

/// Whether the item lays out its children in an inline formatting context.
///
/// Block containers with any inline level child establish one; block children
/// then interrupt the inline content instead of being wrapped in anonymous boxes.
pub(super) fn is_inline_root(tree: &LayoutInputTree, key: LayoutInputKey) -> bool {
    let item = &tree.items[key];

    match item.child_layout {
        ChildLayout::None => item.kind == LayoutInputKind::Text,
        ChildLayout::Inline => true,
        ChildLayout::Block => tree
            .items
            .children(key)
            .any(|child| tree.items[child].kind != LayoutInputKind::Block),
        ChildLayout::Container => false,
    }
}

/// Buffers retained by the builder between builds.
#[derive(Default)]
pub(crate) struct Scratch {
    stack: Vec<LayoutInputKey>,
    /// Box fragments detached from their parents during this build. Those left
    /// without a parent at the end are deleted.
    detached: Vec<FragmentKey>,
    inline: inline::Scratch,
}

pub(crate) struct Engine<'a> {
    pub ui: &'a Ui,
    pub tree: &'a LayoutInputTree,
    pub fragments: &'a mut FragmentTree,
    pub fonts: &'a mut parley::FontContext,
    pub parley: &'a mut parley::LayoutContext<Option<NodeKey>>,
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

    fn resolve_style(&self, key: LayoutInputKey) -> Style {
        let item = &self.tree.items[key];
        let mut cx = self.constraint.clone();
        cx.font_size = item.text_style.span.font_size as f64;

        style::resolve(self.ui, Some(item.node), &cx)
    }

    /// Create the state and box fragment of a box generating item if missing.
    pub fn ensure_state(&mut self, key: LayoutInputKey) {
        let valid = self
            .fragments
            .states
            .get(key)
            .is_some_and(|state| self.fragments.items.get(state.fragment).is_some());

        if valid {
            return;
        }

        let style = self.resolve_style(key);
        let fragment = self.fragments.items.insert(Fragment::new(
            key,
            FragmentItem::Box,
            FragmentBox::default(),
        ));

        self.fragments
            .states
            .insert(key, NodeState::new(style, fragment));
    }

    /// Resolve the box style again and drop cached layouts of a flagged item.
    fn refresh_state(&mut self, key: LayoutInputKey) {
        let style = self.resolve_style(key);
        self.ensure_state(key);

        let state = &mut self.fragments.states[key];
        state.style = style;
        state.cache.clear();
        state.final_layout_valid = false;
    }

    /// Visit flagged items from the root. Cached layouts of flagged items are
    /// dropped and their fragment children are synchronized. Flags are kept.
    pub fn invalidate(&mut self) {
        // Without retained fragments every item is treated as flagged.
        let force = self.fragments.box_fragment(self.root).is_none();
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
                self.invalidate_box(key);
            } else if let Some(state) = self.fragments.states.remove(key) {
                // The item no longer generates a box.
                self.fragments.inline_layouts.remove(key);
                self.release(state.fragment);
            }

            if (force || flags.contains(LayoutFlags::CHILDREN_DIRTY))
                && child_layout != ChildLayout::None
            {
                stack.extend(self.tree.items.children(key));
            }
        }

        self.scratch.stack = stack;
    }

    /// Refresh a flagged box and rebuild its fragment children. Inline content
    /// is attached again when its inline layout is emitted.
    fn invalidate_box(&mut self, key: LayoutInputKey) {
        self.refresh_state(key);

        let fragment = self.fragments.states[key].fragment;
        let inline = is_inline_root(self.tree, key);

        if inline {
            if let Some(layout) = self.fragments.inline_layouts.get_mut(key) {
                layout.shaped = false;
            }
        } else {
            self.fragments.inline_layouts.remove(key);
        }

        self.release_children(fragment);

        if !inline
            && matches!(
                self.tree.items[key].child_layout,
                ChildLayout::Block | ChildLayout::Container
            )
        {
            for child in self.tree.items.children(key) {
                self.ensure_state(child);

                let child = self.fragments.states[child].fragment;
                self.fragments.items.append(fragment, child);
            }
        }
    }

    /// Mark the final layout of the item and its ancestors invalid. Measuring
    /// replaces retained lines and descendant geometry, so the final layout
    /// must run again even when its cache entry matches.
    fn invalidate_final(&mut self, key: LayoutInputKey) {
        let mut ancestor = Some(key);

        while let Some(key) = ancestor {
            if let Some(state) = self.fragments.states.get_mut(key) {
                state.final_layout_valid = false;
            }

            if key == self.root {
                break;
            }

            ancestor = self.tree.items.parent(key);
        }
    }

    /// Lay out a box with the algorithm selected by its children.
    pub fn layout_child(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        let key = from_taffy_key(id);

        if is_inline_root(self.tree, key) {
            return self.layout_inline(key, input);
        }

        match self.tree.items[key].child_layout {
            ChildLayout::Block => taffy::compute_block_layout(self, id, input, block_ctx),
            ChildLayout::Container => self.layout_container(key, input),
            ChildLayout::None | ChildLayout::Inline => self.layout_leaf(key, input),
        }
    }
}
