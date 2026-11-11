//! Splitting inline content interrupted by block boxes into anonymous runs.

use comet_tree::node::Text;

use super::{
    super::{Engine, Fragment, FragmentBox, FragmentItem, FragmentKey, Mode, NodeState},
    events::{InlineEvents, Op},
};
use crate::{
    input::{LayoutFlags, LayoutInputKey},
    taffy::style,
};

impl Engine<'_> {
    /// Whether block boxes interrupt the inline content of an item.
    pub(in crate::fragment::engine) fn has_block_content(&self, key: LayoutInputKey) -> bool {
        InlineEvents::new(self.tree, key).any(|op| matches!(op, Op::Block(_)))
    }

    /// Whether an item in the inline content of an item changed in a way that
    /// requires shaping again. Box style changes of atomic inlines and flags
    /// only propagated from descendants keep the shaped text: atomic inline
    /// metrics are updated without shaping.
    pub(in crate::fragment::engine) fn inline_content_dirty(&self, key: LayoutInputKey) -> bool {
        InlineEvents::new(self.tree, key).any(|op| match op {
            Op::Push(key) | Op::Text(key) | Op::Box(key) | Op::Empty(key) => {
                self.tree.items[key].flags.contains(LayoutFlags::DIRTY)
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
        // Content changes shape a run again; other flags, such as box style
        // changes of atomic inlines, only lay it out again.
        let dirty =
            |key: LayoutInputKey| force || tree.items[key].flags.contains(LayoutFlags::DIRTY);
        let flagged = |key: LayoutInputKey| force || !tree.items[key].flags.is_empty();

        let mut runs: Vec<FragmentKey> = self
            .fragments
            .items
            .children(fragment)
            .filter(|&child| self.fragments.items[child].item == FragmentItem::AnonymousBlock)
            .collect();
        runs.reverse();
        self.release_children(fragment);

        let owner_dirty = force || tree.items[owner].flags.contains(LayoutFlags::DIRTY);
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
                    run.mark(dirty(key), flagged(key));
                    open.push(key);
                }
                Op::Pop => {
                    open.pop();
                }
                Op::Text(key) => {
                    run.content |= ui
                        .prop::<Text>(tree.items[key].node)
                        .is_some_and(|text| !text.0.is_empty());
                    run.mark(dirty(key), flagged(key));
                }
                Op::Box(key) | Op::Empty(key) => {
                    run.content = true;
                    run.mark(dirty(key), flagged(key));
                }
                Op::Block(key) => {
                    if run.content {
                        self.attach_run(owner, fragment, run, runs.pop());
                    }

                    let block = self.ensure_box(key);
                    self.fragments.items.append(fragment, block);

                    let dirty = owner_dirty || open.iter().any(|&key| dirty(key));
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

        let anonymous = match existing {
            Some(anonymous) => {
                let state = &mut self.fragments.states[anonymous];

                if run.dirty || state.mode != mode {
                    state.mode = mode;
                    state.invalidate();

                    if let Some(inline) = self.fragments.inline_layouts.get_mut(anonymous) {
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
                let style = style::resolve(self.ui, None, &self.style_constraint(owner));
                let anonymous = self.fragments.items.insert(Fragment::new(
                    owner,
                    FragmentItem::AnonymousBlock,
                    FragmentBox::default(),
                ));

                self.fragments
                    .states
                    .insert(anonymous, NodeState::new(mode, style));

                anonymous
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
    fn mark(&mut self, dirty: bool, flagged: bool) {
        self.dirty |= dirty;
        self.flagged |= dirty || flagged;
    }
}
