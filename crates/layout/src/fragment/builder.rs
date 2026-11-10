use comet_tree::{node::NodeKey, ui::Ui};

use super::{
    FragmentKey,
    engine::{Engine, Scratch},
    tree::FragmentTree,
};
use crate::{
    input::{LayoutInputKey, tree::LayoutInputTree},
    style::LayoutConstraint,
    taffy::conv,
};

/// Builds a [`FragmentTree`] from a [`LayoutInputTree`], updating retained
/// fragments of clean subtrees in place.
#[derive(Default)]
pub struct FragmentTreeBuilder {
    pub fonts: parley::FontContext,
    parley: parley::LayoutContext<Option<NodeKey>>,
    scratch: Scratch,
}

impl FragmentTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lay out `root` and update its fragments, or build them when `fragments`
    /// has none. Returns the box fragment of `root`.
    ///
    /// Starting from `root`, flagged items are visited and laid out again.
    /// Clean subtrees reuse their cached layouts and fragments. Flags are not
    /// cleared; call [`LayoutInputTree::clear_flags`] after building.
    ///
    /// `constraint` supplies the viewport and root font size used to resolve
    /// box styles, and the space available to `root`. Mark the input tree dirty
    /// (for example with a forced input build) when the viewport or root font
    /// size changes. Returns `None` and deletes the retained fragments if `root`
    /// is no longer in the input tree.
    ///
    /// Box fragments of block level and atomic inline items, and anonymous
    /// blocks of unchanged runs, keep their keys across builds. Line, inline and
    /// glyph run fragments of laid out lines are replaced.
    pub fn build(
        &mut self,
        ui: &Ui,
        tree: &LayoutInputTree,
        root: LayoutInputKey,
        fragments: &mut FragmentTree,
        constraint: &LayoutConstraint,
    ) -> Option<FragmentKey> {
        let mut engine = Engine {
            ui,
            tree,
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
        engine
            .fragments
            .constraints
            .insert(fragment, constraint.clone());

        #[cfg(debug_assertions)]
        engine.check();

        Some(fragment)
    }
}
