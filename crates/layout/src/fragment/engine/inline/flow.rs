//! Splitting inline content interrupted by block boxes into anonymous runs.

use comet_tree::node::Text;

use super::{
    super::{Engine, FragmentItem, FragmentKey, Mode, NodeState, content_changed, layout_changed},
    events::{InlineEvents, Op},
};
use crate::{
    dirty_map::DirtyMap,
    input::{InputFlags, LayoutInputKey},
    taffy::style,
};

impl Engine<'_> {
    /// Whether block boxes interrupt the inline content of an item.
    pub(in crate::fragment::engine) fn has_block_content(&self, key: LayoutInputKey) -> bool {
        InlineEvents::new(self.tree, key).any(|op| matches!(op, Op::Block(_)))
    }

    /// Push the atomic inline and block boxes inside the inline elements of an
    /// item's inline content. Boxes directly under the item are its children.
    pub(in crate::fragment::engine) fn push_nested_inline_boxes(
        &self,
        owner: LayoutInputKey,
        out: &mut Vec<LayoutInputKey>,
    ) {
        out.extend(
            InlineEvents::new(self.tree, owner).filter_map(|op| match op {
                Op::Box(key) | Op::Block(key) if self.tree.items.parent(key) != Some(owner) => {
                    Some(key)
                }
                _ => None,
            }),
        );
    }

    /// Whether an item in the inline content of an item changed in a way that
    /// requires shaping again. Box style changes of atomic inlines and flags
    /// only propagated from descendants keep the shaped text: atomic inline
    /// metrics are updated without shaping.
    pub(in crate::fragment::engine) fn inline_content_dirty(&self, key: LayoutInputKey) -> bool {
        InlineEvents::new(self.tree, key).any(|op| match op {
            Op::Push(key) | Op::Text(key) | Op::Box(key) | Op::Empty(key) => {
                content_changed(self.dirty.entry(key).flags)
            }
            Op::Block(_) | Op::Pop => false,
        })
    }

    /// Attach the anonymous blocks and block boxes of mixed inline content to
    /// the owner's box, in flow order.
    ///
    /// Runs without content are not created, so the margins of adjacent block
    /// boxes collapse. Existing anonymous blocks are reused in order; a run is
    /// shaped again when its content, its start or the owner changed.
    pub(in crate::fragment::engine) fn rebuild_runs(
        &mut self,
        owner: LayoutInputKey,
        fragment: FragmentKey,
        force: bool,
    ) {
        let tree = self.tree;
        let ui = self.ui;

        let mut runs: Vec<FragmentKey> = self
            .fragments
            .items
            .children(fragment)
            .filter(|&child| self.fragments.items[child].item.is_anonymous_block())
            .collect();
        runs.reverse();
        self.release_children(fragment);

        let owner_dirty = force || content_changed(self.dirty.entry(owner).flags);
        // Inline elements open at the current position, which runs reopen.
        let mut open: Vec<LayoutInputKey> = Vec::new();
        let mut run = Run {
            after: None,
            content: false,
            dirty: owner_dirty,
            flagged: owner_dirty,
        };

        for op in InlineEvents::new(tree, owner) {
            match op {
                Op::Push(key) => {
                    run.mark_item(self.dirty, key, force);
                    open.push(key);
                }
                Op::Pop => {
                    open.pop();
                }
                Op::Text(key) => {
                    run.content |= ui
                        .prop::<Text>(tree.items[key].node)
                        .is_some_and(|text| !text.0.is_empty());
                    run.mark_item(self.dirty, key, force);
                }
                Op::Box(key) | Op::Empty(key) => {
                    run.content = true;
                    run.mark_item(self.dirty, key, force);
                }
                Op::Block(key) => {
                    if run.content {
                        self.attach_run(owner, fragment, run, runs.pop());
                    }

                    let block = self.ensure_box(key);
                    self.fragments.items.append(fragment, block);

                    let dirty = owner_dirty
                        || open
                            .iter()
                            .any(|&key| force || content_changed(self.dirty.entry(key).flags));
                    run = Run {
                        after: Some(key),
                        content: false,
                        dirty,
                        flagged: dirty,
                    };
                }
            }
        }

        if run.content {
            self.attach_run(owner, fragment, run, runs.pop());
        }
    }

    /// Append the anonymous block of a run, reusing `existing` when given.
    fn attach_run(
        &mut self,
        owner: LayoutInputKey,
        fragment: FragmentKey,
        run: Run,
        existing: Option<FragmentKey>,
    ) {
        let mode = Mode::Run { after: run.after };

        let reused = existing.filter(|&anonymous| self.fragments.get_state(anonymous).is_some());

        let anonymous = match reused {
            Some(anonymous) => {
                let Some(state) = self.fragments.get_state_mut(anonymous) else {
                    return;
                };

                if run.dirty || state.mode != mode {
                    state.mode = mode;
                    state.invalidate();

                    if let Some(inline) = state.inline.as_deref_mut() {
                        inline.shaped = false;
                    }
                } else if run.flagged {
                    // Atomic inline sizes may change; their metrics are
                    // updated when the run is laid out again.
                    state.invalidate();
                }

                anonymous
            }
            None => {
                let style = style::resolve(None, &self.style_constraint(owner));
                self.dirty
                    .mark(self.tree, owner, InputFlags::NEED_PRE_PAINT);
                self.fragments.insert_with_state(
                    owner,
                    |state| FragmentItem::AnonymousBlock { state },
                    NodeState::new(mode, style),
                )
            }
        };

        self.fragments.items.append(fragment, anonymous);
    }
}

/// A run of inline content between block boxes.
struct Run {
    /// The block box preceding the run.
    after: Option<LayoutInputKey>,
    /// Whether the run has text, atomic inlines or empty inline elements.
    content: bool,
    /// Whether the run must be shaped again.
    dirty: bool,
    /// Whether the run must be laid out again.
    flagged: bool,
}

impl Run {
    /// Mark the run with the flags of an item in it. Content changes shape
    /// the run again; other flags, such as box style changes of atomic
    /// inlines, only lay it out again. `force` treats the item as changed.
    fn mark_item(&mut self, dirty: &DirtyMap, key: LayoutInputKey, force: bool) {
        let flags = dirty.entry(key).flags;

        self.dirty |= force || content_changed(flags);
        self.flagged |= force || content_changed(flags) || layout_changed(flags);
    }
}
