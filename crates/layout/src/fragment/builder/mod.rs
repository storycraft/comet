//! Building a [`FragmentTree`] from a layout input tree, reflecting the
//! changes marked in a [`DirtyMap`].
//!
//! A build runs in steps:
//! 1. [`children`]: boxes are added, removed, moved and changed where
//!    [`DirtyFlags::NEED_CHILDREN`] is marked, without laying anything out.
//!    Inline formatting contexts whose content changed are marked to be
//!    built again. Box keys are kept.
//! 2. [`invalidate`]: boxes marked [`DirtyFlags::NEED_BOX_LAYOUT`], or
//!    depending on a changed viewport, resolve their styles again and drop
//!    their cached layouts and those of their ancestors.
//! 3. [`block`]: boxes are laid out with Taffy. An inline formatting
//!    context is [built](content) in place when it is laid out, with the boxes
//!    inside it, then laid out by breaking its [`lines`] and
//!    [emitting](emit) its items.

mod block;
#[cfg(debug_assertions)]
mod check;
mod children;
pub mod classify;
mod content;
mod emit;
mod invalidate;
mod lines;
pub(crate) mod taffy;

use comet_content::manager::ContentManager;
use comet_tree::ui::Ui;
use rustc_hash::FxHashMap;
use slotmap::SecondaryMap;

use super::{
    BoxFragmentKey,
    inline::collect::{FlowItem, InlineBuildContext, InlineContentBuilder},
    tree::FragmentTree,
};
use crate::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::NodeLayoutBits,
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::LayoutConstraint,
};

/// Flags of an item whose fragments were added, moved or changed, so
/// pre-paint and layering visit it again.
pub(crate) const PAINT_CHANGED: DirtyFlags =
    DirtyFlags::NEED_PRE_PAINT.union(DirtyFlags::NEED_LAYER);

/// Builds and updates [`FragmentTree`]s. Keep it to reuse its buffers.
/// Fonts come from a [`parley::FontContext`] its owner passes to each build.
#[derive(Default)]
pub struct FragmentTreeBuilder {
    content: InlineContentBuilder,
    scratch: Scratch,
}

/// Buffers retained between builds.
#[derive(Default)]
struct Scratch {
    /// Pending work of the child sync.
    tasks: Vec<Task>,
    /// Boxes created by the current build.
    created: SecondaryMap<BoxFragmentKey, ()>,
    /// Boxes created by the current build, in creation order.
    created_order: Vec<BoxFragmentKey>,
    /// Keys collected while borrowing the fragment tree.
    keys: Vec<(LayoutInputKey, BoxFragmentKey)>,
    /// Items to visit by the invalidation, with the box of their nearest
    /// ancestor generating one, or their own box.
    walk: Vec<(LayoutInputKey, BoxFragmentKey)>,
    /// Stack of the child boxes of the boxes being laid out.
    children: Vec<BoxFragmentKey>,
    /// Out-of-flow boxes being placed.
    hoisted: Vec<BoxFragmentKey>,
    /// Boxes inside the inline content being laid out.
    inline_children: Vec<BoxFragmentKey>,
    /// Runs and block level boxes of a mixed inline box being reconciled.
    flow: Vec<FlowItem>,
    /// Run boxes of a mixed inline box being reconciled, by the block they
    /// follow.
    runs: Vec<(Option<LayoutInputKey>, BoxFragmentKey)>,
    /// Boxes detached for a later reconcile in this build, see
    /// [`children`].
    parked: Vec<BoxFragmentKey>,
    /// Index of the latest emitted inline item of each source.
    links: FxHashMap<LayoutInputKey, usize>,
    /// Inline element pieces open on the line being emitted.
    open: Vec<emit::Piece>,
    /// Inline ancestors of the inline item being emitted.
    chain: Vec<LayoutInputKey>,
    /// Out-of-flow candidates left by the latest final layout of each box
    /// inside the inline content being laid out.
    child_candidates: FxHashMap<BoxFragmentKey, ::taffy::OofCandidates>,
    /// Out-of-flow candidates of the inline content being emitted.
    candidates: ::taffy::OofCandidates,
    /// Index of each child among the children of the box being emitted.
    order: FxHashMap<BoxFragmentKey, u32>,
    /// Inline part boxes of the box being emitted, in emit order.
    placed_pieces: Vec<BoxFragmentKey>,
    /// Mixed inline boxes whose runs were emitted, to link the pieces of the
    /// elements split by their blocks.
    relink: Vec<BoxFragmentKey>,
}

/// Pending work of the child sync.
enum Task {
    /// Sync the attached box of an item with the changes marked in its
    /// subtree.
    Sync(LayoutInputKey, BoxFragmentKey),
    /// Append fresh boxes for the children of a new block or container box.
    Children(LayoutInputKey, BoxFragmentKey),
}

impl FragmentTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run the steps of [`Self::prepare`] before laying out: sync the box
    /// structure and invalidate the affected caches.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update(
        &mut self,
        ui: &Ui,
        tree: &LayoutInputTree,
        contents: &ContentManager,
        fonts: &mut parley::FontContext,
        dirty: &mut DirtyMap,
        root: LayoutInputKey,
        fragments: &mut FragmentTree,
        constraint: &LayoutConstraint,
    ) -> Option<BoxFragmentKey> {
        self.run(
            ui, tree, contents, fonts, dirty, root, fragments, constraint, false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        ui: &Ui,
        tree: &LayoutInputTree,
        contents: &ContentManager,
        fonts: &mut parley::FontContext,
        dirty: &mut DirtyMap,
        root: LayoutInputKey,
        fragments: &mut FragmentTree,
        constraint: &LayoutConstraint,
        layout: bool,
    ) -> Option<BoxFragmentKey> {
        tree.items.get(root)?;
        self.scratch.created.clear();
        self.scratch.created_order.clear();

        let mut engine = Engine {
            cx: InlineBuildContext {
                ui,
                tree,
                constraint,
            },
            fonts,
            content: &mut self.content,
            contents,
            dirty,
            fragments,
            scratch: &mut self.scratch,
            root,
            root_box: None,
        };

        let key = engine.sync_children();
        engine.invalidate();

        if layout {
            engine.layout(key);
            engine.link_split_pieces();
            // Left by boxes that were laid out but not placed.
            engine.scratch.child_candidates.clear();
        }

        engine.delete_parked();

        #[cfg(debug_assertions)]
        if layout {
            check::check(&engine.cx, engine.fragments, key);
        }

        Some(key)
    }
}

#[bon::bon]
impl FragmentTreeBuilder {
    /// Update the box fragments of `root` in `fragments` with the changes
    /// marked in `dirty` and lay them out, returning the box of `root`. Box
    /// keys are kept for as long as their items are. Returns `None` for a
    /// stale root; the caller deletes the box it built for it before.
    ///
    /// In debug builds, the boxes of `root` are checked once laid out.
    ///
    /// Items whose fragments changed are marked [`PAINT_CHANGED`]. Flags are
    /// not cleared.
    ///
    /// Boxes whose layout is still valid are not laid out again.
    ///
    /// Boxes displaying a content, with `DisplayInner::Content`, are sized by
    /// the [`Content`](comet_content::Content) of their id in `contents`, or
    /// by their style alone when it is missing.
    ///
    /// Text is shaped with the fonts of `fonts`.
    #[builder(finish_fn = build)]
    pub fn prepare(
        &mut self,
        ui: &Ui,
        tree: &LayoutInputTree,
        contents: &ContentManager,
        fonts: &mut parley::FontContext,
        dirty: &mut DirtyMap,
        root: LayoutInputKey,
        fragments: &mut FragmentTree,
        constraint: &LayoutConstraint,
    ) -> Option<BoxFragmentKey> {
        self.run(
            ui, tree, contents, fonts, dirty, root, fragments, constraint, true,
        )
    }
}

/// Borrowed state of one build.
pub(crate) struct Engine<'a> {
    cx: InlineBuildContext<'a>,
    fonts: &'a mut parley::FontContext,
    content: &'a mut InlineContentBuilder,
    /// Contents sizing the boxes displaying them.
    contents: &'a ContentManager,
    dirty: &'a mut DirtyMap,
    fragments: &'a mut FragmentTree,
    scratch: &'a mut Scratch,
    root: LayoutInputKey,
    /// The box of the root, once known.
    root_box: Option<BoxFragmentKey>,
}

impl Engine<'_> {
    fn tree(&self) -> &LayoutInputTree {
        self.cx.tree
    }

    /// Drop the cached layouts of a box and its ancestors. Stops at the
    /// first ancestor without cached layouts: an invalid box never has a
    /// valid ancestor.
    fn invalidate_up(&mut self, key: BoxFragmentKey) {
        let Some(fragment) = self.fragments.get_mut(key) else {
            return;
        };
        fragment.invalidate();

        let mut current = self.fragments.boxes.parent(key);
        while let Some(key) = current {
            let fragment = &mut self.fragments.boxes[key];

            if !fragment.final_layout_valid && fragment.cache.is_empty() {
                return;
            }

            fragment.invalidate();
            current = self.fragments.boxes.parent(key);
        }
    }

    /// Add the [propagated](NodeLayoutBits::PROPAGATED) part of `bits` of
    /// the box `key` to its ancestors.
    fn propagate_bits(&mut self, key: BoxFragmentKey, bits: NodeLayoutBits) {
        let bits = bits & NodeLayoutBits::PROPAGATED;
        if bits.is_empty() {
            return;
        }

        let mut current = self.fragments.boxes.parent(key);
        while let Some(ancestor) = current {
            let fragment = &mut self.fragments.boxes[ancestor];

            // Bits always reach the root, so ancestors above have them.
            if fragment.bits.contains(bits) {
                break;
            }

            fragment.bits |= bits;
            current = self.fragments.boxes.parent(ancestor);
        }
    }

    fn mark_paint(&mut self, source: LayoutInputKey) {
        let tree = self.cx.tree;
        self.dirty.mark(tree, source, PAINT_CHANGED);
    }
}
