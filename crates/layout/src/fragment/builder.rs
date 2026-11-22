use comet_tree::ui::Ui;

use super::{
    FragmentKey,
    engine::{Engine, Scratch},
    tree::FragmentTree,
};
use crate::{
    dirty_map::DirtyMap,
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::LayoutConstraint,
    taffy::conv,
};

/// Builds a [`FragmentTree`] from a [`LayoutInputTree`], updating retained
/// fragments of clean subtrees in place.
#[derive(Default)]
pub struct FragmentTreeBuilder {
    pub fonts: parley::FontContext,
    parley: parley::LayoutContext<Option<LayoutInputKey>>,
    scratch: Scratch,
}

impl FragmentTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lay out `root` and update its fragments, or build them when `fragments`
    /// has none. Returns the box fragment of `root`.
    ///
    /// Items flagged in `dirty` are laid out again. Flags are not cleared;
    /// call [`DirtyMap::clear`] after building.
    ///
    /// Items whose fragments were created, moved or resized are marked
    /// [`DirtyFlags::NEED_PRE_PAINT`](crate::dirty_map::DirtyFlags::NEED_PRE_PAINT)
    /// and [`DirtyFlags::NEED_LAYER`](crate::dirty_map::DirtyFlags::NEED_LAYER)
    /// in `dirty`.
    ///
    /// `constraint` supplies the viewport and root font size, and the space
    /// available to `root`. Build the input tree again when the viewport or
    /// root font size changes. Returns `None` and deletes the retained
    /// fragments if `root` is no longer in the input tree.
    ///
    /// Box fragments keep their keys across builds.
    pub fn build(
        &mut self,
        ui: &Ui,
        tree: &LayoutInputTree,
        dirty: &mut DirtyMap,
        root: LayoutInputKey,
        fragments: &mut FragmentTree,
        constraint: &LayoutConstraint,
    ) -> Option<FragmentKey> {
        let mut engine = Engine {
            ui,
            tree,
            dirty,
            fragments,
            fonts: &mut self.fonts,
            parley: &mut self.parley,
            constraint,
            root,
            scratch: &mut self.scratch,
        };

        if tree.items.get(root).is_none() {
            if let Some(&fragment) = engine.fragments.first.get(root)
                && engine.fragments.items.get(fragment).is_some()
            {
                engine.drop_state(fragment);
                engine.release(fragment);
                engine.sweep();
            }

            return None;
        }

        engine.invalidate();

        let fragment = engine.ensure_box(root);
        engine.compute(
            fragment,
            conv::to_size(constraint.available_size, |v| {
                taffy::AvailableSpace::Definite(v as f32)
            }),
        );
        engine.sweep();

        #[cfg(debug_assertions)]
        engine.check();

        Some(fragment)
    }
}
