use taffy::{CacheTree, LayoutPartialTree, TraversePartialTree};

use super::{
    Fragment, FragmentKey,
    engine::{Engine, fragment_of, node_id},
    state::Mode,
};
use crate::{
    slot_tree::iterator::Cursor,
    style::geometry::{Point, Size},
    taffy::{conv, style::Style},
};

/// Borrowed child traversal for Taffy; nodes without block children have no
/// iterator.
pub struct Children<'a>(Option<Cursor<'a, FragmentKey, Fragment>>);

impl Iterator for Children<'_> {
    type Item = taffy::NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.as_mut()?.next().map(node_id)
    }
}

impl TraversePartialTree for Engine<'_> {
    type ChildIter<'a>
        = Children<'a>
    where
        Self: 'a;

    /// Block containers expose their fragment children: block boxes, and the
    /// anonymous blocks of mixed inline content. Lines are measured by Parley.
    fn child_ids(&self, id: taffy::NodeId) -> Self::ChildIter<'_> {
        let fragment = fragment_of(id);
        let block = matches!(
            self.fragments.states[fragment].mode,
            Mode::Block | Mode::MixedInline
        );

        Children(block.then(|| self.fragments.items.children(fragment)))
    }

    fn child_count(&self, id: taffy::NodeId) -> usize {
        self.child_ids(id).count()
    }

    fn get_child_id(&self, id: taffy::NodeId, index: usize) -> taffy::NodeId {
        self.child_ids(id).nth(index).unwrap()
    }
}

impl LayoutPartialTree for Engine<'_> {
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type CustomIdent = String;

    fn get_core_container_style(&self, id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        &self.fragments.states[fragment_of(id)].style
    }

    /// Block layouts need no inheritance, so they are converted into their
    /// fragments in place.
    fn set_unrounded_layout(&mut self, id: taffy::NodeId, layout: &taffy::Layout) {
        let key = fragment_of(id);
        self.fragments.states[key].layout = *layout;

        let fragment = &mut self.fragments.items[key].layout;
        fragment.location = Point::new(layout.location.x as f64, layout.location.y as f64);
        fragment.size = Size::new(layout.size.width as f64, layout.size.height as f64);
        fragment.padding = conv::from_rect(layout.padding, f64::from);
        fragment.border = conv::from_rect(layout.border, f64::from);
    }

    fn compute_child_layout(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        taffy::compute_cached_layout(self, id, input, |this, id, input| {
            this.layout_child(id, input, None)
        })
    }
}

impl taffy::LayoutBlockContainer for Engine<'_> {
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
        taffy::compute_cached_layout(self, id, input, |this, id, input| {
            this.layout_child(id, input, block_ctx)
        })
    }
}

impl taffy::LayoutContainingBlock for Engine<'_> {
    type OofItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_oof_item_style(&self, id: taffy::NodeId) -> Self::OofItemStyle<'_> {
        self.get_core_container_style(id)
    }

    fn clear_hoisted_children(&mut self, id: taffy::NodeId) {
        self.fragments.states[fragment_of(id)].hoisted.clear();
    }

    fn add_hoisted_children(&mut self, id: taffy::NodeId, children: &[taffy::NodeId]) {
        self.fragments.states[fragment_of(id)]
            .hoisted
            .extend_from_slice(children);
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
