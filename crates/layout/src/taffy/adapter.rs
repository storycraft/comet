use taffy::{CacheTree, LayoutPartialTree, TraversePartialTree};

use super::{TaffyLayout, from_taffy_key, to_taffy_key};
use crate::{
    slot_tree::iterator::Cursor,
    tree::{ChildLayoutMode, LayoutItem, LayoutItemKey},
};

/// Borrowed child traversal for Taffy; leaf nodes have no iterator.
pub struct Children<'a>(Option<Cursor<'a, LayoutItemKey, LayoutItem>>);

impl Iterator for Children<'_> {
    type Item = taffy::NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.as_mut()?.next().map(to_taffy_key)
    }
}

impl TraversePartialTree for TaffyLayout<'_> {
    type ChildIter<'a>
        = Children<'a>
    where
        Self: 'a;

    fn child_ids(&self, id: taffy::NodeId) -> Self::ChildIter<'_> {
        let key = from_taffy_key(id);

        Children(
            matches!(self.tree.items[key].child_layout, ChildLayoutMode::Block)
                .then(|| self.tree.items.children(key)),
        )
    }

    fn child_count(&self, id: taffy::NodeId) -> usize {
        self.child_ids(id).count()
    }

    fn get_child_id(&self, id: taffy::NodeId, index: usize) -> taffy::NodeId {
        self.child_ids(id).nth(index).unwrap()
    }
}

impl LayoutPartialTree for TaffyLayout<'_> {
    type CoreContainerStyle<'a>
        = &'a crate::taffy::style::Style
    where
        Self: 'a;
    type CustomIdent = String;

    fn get_core_container_style(&self, id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        &self.nodes[&from_taffy_key(id)].style
    }

    fn set_unrounded_layout(&mut self, id: taffy::NodeId, layout: &taffy::Layout) {
        self.nodes.get_mut(&from_taffy_key(id)).unwrap().layout = *layout;
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

impl taffy::LayoutBlockContainer for TaffyLayout<'_> {
    type BlockContainerStyle<'a>
        = &'a crate::taffy::style::Style
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a crate::taffy::style::Style
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

impl taffy::LayoutContainingBlock for TaffyLayout<'_> {
    type OofItemStyle<'a>
        = &'a crate::taffy::style::Style
    where
        Self: 'a;

    fn get_oof_item_style(&self, id: taffy::NodeId) -> Self::OofItemStyle<'_> {
        self.get_core_container_style(id)
    }

    fn clear_hoisted_children(&mut self, id: taffy::NodeId) {
        self.nodes
            .get_mut(&from_taffy_key(id))
            .unwrap()
            .hoisted
            .clear();
    }

    fn add_hoisted_children(&mut self, id: taffy::NodeId, children: &[taffy::NodeId]) {
        self.nodes
            .get_mut(&from_taffy_key(id))
            .unwrap()
            .hoisted
            .extend_from_slice(children);
    }
}

impl CacheTree for TaffyLayout<'_> {
    fn cache_get(
        &mut self,
        id: taffy::NodeId,
        input: &taffy::LayoutInput,
    ) -> Option<taffy::LayoutOutput> {
        let state = self.nodes.get_mut(&from_taffy_key(id)).unwrap();

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
        let state = self.nodes.get_mut(&from_taffy_key(id)).unwrap();
        state.cache.store(input, output);

        if input.run_mode == taffy::RunMode::PerformLayout {
            state.final_layout_valid = true;
        }
    }

    fn cache_clear(&mut self, id: taffy::NodeId) {
        self.nodes
            .get_mut(&from_taffy_key(id))
            .unwrap()
            .cache
            .clear();
    }
}
