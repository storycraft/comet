use std::ops::{Deref, DerefMut, Range};

use taffy::{CacheTree, LayoutPartialTree, TraversePartialTree};

use super::{
    FragmentKey,
    engine::{Engine, fragment_of, node_id},
    state::Mode,
};
use crate::{
    style::geometry::{Point, Size},
    taffy::{conv, style::Style},
};

/// Taffy tree for laying out one fragment: the fragment and its direct
/// children, buffered for constant time indexed access.
///
/// Children live in a stack buffer shared by nested layouts. Layouts of
/// descendants push their children after this range and truncate back to it
/// when done, so the buffer is reused across the whole build.
pub(crate) struct FragmentEngine<'e, 'a> {
    engine: &'e mut Engine<'a>,
    fragment: FragmentKey,
    /// Range of the direct children in `scratch.children`.
    children: Range<usize>,
}

impl<'e, 'a> FragmentEngine<'e, 'a> {
    /// Block containers expose their fragment children: block boxes, and the
    /// anonymous blocks of mixed inline content. Lines are measured by Parley.
    /// Custom containers expose their child boxes.
    pub fn new(engine: &'e mut Engine<'a>, fragment: FragmentKey) -> Self {
        let start = engine.scratch.children.len();

        if matches!(
            engine.fragments.states[fragment].mode,
            Mode::Block | Mode::MixedInline | Mode::Container
        ) {
            engine
                .scratch
                .children
                .extend(engine.fragments.items.children(fragment));
        }

        let end = engine.scratch.children.len();

        Self {
            engine,
            fragment,
            children: start..end,
        }
    }

    pub fn fragment(&self) -> FragmentKey {
        self.fragment
    }

    /// Direct children of the fragment, in tree order.
    pub fn children(&self) -> &[FragmentKey] {
        &self.engine.scratch.children[self.children.clone()]
    }

    fn children_of(&self, id: taffy::NodeId) -> &[FragmentKey] {
        debug_assert_eq!(
            fragment_of(id),
            self.fragment,
            "only the laid out fragment is traversed"
        );

        self.children()
    }
}

impl Drop for FragmentEngine<'_, '_> {
    fn drop(&mut self) {
        self.engine.scratch.children.truncate(self.children.start);
    }
}

impl<'a> Deref for FragmentEngine<'_, 'a> {
    type Target = Engine<'a>;

    fn deref(&self) -> &Self::Target {
        self.engine
    }
}

impl DerefMut for FragmentEngine<'_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.engine
    }
}

impl TraversePartialTree for FragmentEngine<'_, '_> {
    type ChildIter<'a>
        = std::iter::Map<std::slice::Iter<'a, FragmentKey>, fn(&FragmentKey) -> taffy::NodeId>
    where
        Self: 'a;

    fn child_ids(&self, id: taffy::NodeId) -> Self::ChildIter<'_> {
        self.children_of(id).iter().map(|&child| node_id(child))
    }

    fn child_count(&self, id: taffy::NodeId) -> usize {
        self.children_of(id).len()
    }

    fn get_child_id(&self, id: taffy::NodeId, index: usize) -> taffy::NodeId {
        node_id(self.children_of(id)[index])
    }
}

impl LayoutPartialTree for FragmentEngine<'_, '_> {
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type CustomIdent = String;

    fn get_core_container_style(&self, id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        &self.engine.fragments.states[fragment_of(id)].style
    }

    fn set_unrounded_layout(&mut self, id: taffy::NodeId, layout: &taffy::Layout) {
        self.engine.set_unrounded_layout(id, layout);
    }

    fn compute_child_layout(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        taffy::compute_cached_layout(self.engine, id, input, |engine, id, input| {
            engine.layout_child(id, input, None)
        })
    }
}

impl taffy::LayoutBlockContainer for FragmentEngine<'_, '_> {
    type BlockContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_block_container_style(&self, id: taffy::NodeId) -> Self::BlockContainerStyle<'_> {
        self.get_core_container_style(id)
    }

    fn get_block_child_style(&self, id: taffy::NodeId) -> Self::BlockItemStyle<'_> {
        self.get_core_container_style(id)
    }

    fn compute_block_child_layout(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        taffy::compute_cached_layout(self.engine, id, input, |engine, id, input| {
            engine.layout_child(id, input, block_ctx)
        })
    }
}

impl taffy::LayoutContainingBlock for FragmentEngine<'_, '_> {
    type OofItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_oof_item_style(&self, id: taffy::NodeId) -> Self::OofItemStyle<'_> {
        self.get_core_container_style(id)
    }

    fn clear_hoisted_children(&mut self, id: taffy::NodeId) {
        self.engine.fragments.states[fragment_of(id)]
            .hoisted
            .clear();
    }

    fn add_hoisted_children(&mut self, id: taffy::NodeId, children: &[taffy::NodeId]) {
        self.engine.fragments.states[fragment_of(id)]
            .hoisted
            .extend_from_slice(children);
    }
}

impl CacheTree for FragmentEngine<'_, '_> {
    fn cache_get(
        &mut self,
        id: taffy::NodeId,
        input: &taffy::LayoutInput,
    ) -> Option<taffy::LayoutOutput> {
        self.engine.cache_get(id, input)
    }

    fn cache_store(
        &mut self,
        id: taffy::NodeId,
        input: &taffy::LayoutInput,
        output: taffy::LayoutOutput,
    ) {
        self.engine.cache_store(id, input, output);
    }

    fn cache_clear(&mut self, id: taffy::NodeId) {
        self.engine.cache_clear(id);
    }
}

impl Engine<'_> {
    /// Block layouts need no inheritance, so they are converted into their
    /// fragments in place.
    pub fn set_unrounded_layout(&mut self, id: taffy::NodeId, layout: &taffy::Layout) {
        let key = fragment_of(id);
        self.fragments.states[key].layout = *layout;

        let fragment = &mut self.fragments.items[key].layout;
        fragment.location = Point::new(layout.location.x as f64, layout.location.y as f64);
        fragment.size = Size::new(layout.size.width as f64, layout.size.height as f64);
        fragment.padding = conv::from_rect(layout.padding, f64::from);
        fragment.border = conv::from_rect(layout.border, f64::from);
    }
}

impl CacheTree for Engine<'_> {
    fn cache_get(
        &mut self,
        id: taffy::NodeId,
        input: &taffy::LayoutInput,
    ) -> Option<taffy::LayoutOutput> {
        let state = &mut self.fragments.states[fragment_of(id)];

        if input.run_mode == taffy::RunMode::PerformLayout && !state.final_layout_valid {
            return None;
        }

        state.cache.get(input)
    }

    fn cache_store(
        &mut self,
        id: taffy::NodeId,
        input: &taffy::LayoutInput,
        output: taffy::LayoutOutput,
    ) {
        let state = &mut self.fragments.states[fragment_of(id)];
        state.cache.store(input, output);

        if input.run_mode == taffy::RunMode::PerformLayout {
            state.final_layout_valid = true;
        }
    }

    fn cache_clear(&mut self, id: taffy::NodeId) {
        self.fragments.states[fragment_of(id)].cache.clear();
    }
}
