//! Building a [`FragmentTree`] from a layout input tree, reflecting the
//! changes marked in a [`DirtyMap`].
//!
//! A build runs in steps:
//! 1. [`children`]: boxes are added, removed, moved and rebuilt where
//!    [`DirtyFlags::NEED_CHILDREN`] is marked, without laying anything out.
//!    Inline formatting contexts whose content changed are built again with
//!    their text shaped.
//! 2. Caches of boxes marked [`DirtyFlags::NEED_BOX_LAYOUT`] are dropped.
//!    Not implemented yet.
//! 3. Boxes and inline layouts are laid out. Not implemented yet.

mod children;
pub mod classify;

use comet_tree::ui::Ui;
use slotmap::SecondaryMap;

use super::{
    BoxFragmentKey, FragmentTree,
    inline::builder::{InlineBuildContext, InlineFragmentainer, InlineLayoutBuilder},
};
use crate::{
    dirty_map::{DirtyFlags, DirtyMap},
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::LayoutConstraint,
};

/// Flags of an item whose fragments were added, moved or changed, so
/// pre-paint and layering visit it again.
pub(crate) const PAINT_CHANGED: DirtyFlags =
    DirtyFlags::NEED_PRE_PAINT.union(DirtyFlags::NEED_LAYER);

/// Builds and updates [`FragmentTree`]s. Keep it to reuse its buffers.
#[derive(Default)]
pub struct FragmentTreeBuilder {
    pub fonts: parley::FontContext,
    inline: InlineLayoutBuilder,
    scratch: Scratch,
}

/// Buffers retained between builds.
#[derive(Default)]
struct Scratch {
    /// Pending work of the child sync.
    tasks: Vec<Task>,
    /// Boxes created by the current build.
    created: SecondaryMap<BoxFragmentKey, ()>,
    /// Keys collected while borrowing the fragment tree.
    keys: Vec<(LayoutInputKey, BoxFragmentKey)>,
}

/// Pending work of the child sync.
enum Task {
    /// Sync the attached box of an item with the changes marked in its
    /// subtree.
    Sync(LayoutInputKey, BoxFragmentKey),
    /// Append fresh boxes for the children of a new block or container box.
    Children(LayoutInputKey, BoxFragmentKey),
    /// Build and attach the boxes inside the inline content of a box built
    /// by the inline layout builder.
    Inline(InlineFragmentainer),
}

impl FragmentTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the box fragments of `root` in `fragments` with the changes
    /// marked in `dirty`, returning the box of `root`. Box keys are kept,
    /// except for those of rebuilt boxes, which includes the root box when
    /// it is rebuilt. Returns `None` for a stale root; the caller deletes the
    /// box it built for it before.
    ///
    /// Items whose fragments changed are marked [`PAINT_CHANGED`]. Flags are
    /// not cleared.
    ///
    /// Only the box structure is updated for now: boxes are not laid out.
    pub fn build(
        &mut self,
        ui: &Ui,
        tree: &LayoutInputTree,
        dirty: &mut DirtyMap,
        root: LayoutInputKey,
        fragments: &mut FragmentTree,
        constraint: &LayoutConstraint,
    ) -> Option<BoxFragmentKey> {
        tree.items.get(root)?;
        self.scratch.created.clear();

        let mut engine = Engine {
            cx: InlineBuildContext {
                ui,
                tree,
                fonts: &mut self.fonts,
                constraint,
            },
            dirty,
            fragments,
            inline: &mut self.inline,
            scratch: &mut self.scratch,
            root,
            root_box: None,
        };

        let key = engine.sync_children();
        self.inline.release();

        Some(key)
    }
}

/// Borrowed state of one build.
struct Engine<'a> {
    cx: InlineBuildContext<'a>,
    dirty: &'a mut DirtyMap,
    fragments: &'a mut FragmentTree,
    inline: &'a mut InlineLayoutBuilder,
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

    fn mark_paint(&mut self, source: LayoutInputKey) {
        let tree = self.cx.tree;
        self.dirty.mark(tree, source, PAINT_CHANGED);
    }
}
