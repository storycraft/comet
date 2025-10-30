use slotmap::KeyData;
use taffy::{
    CacheTree, LayoutBlockContainer, LayoutPartialTree, RoundTree, TraversePartialTree,
    TraverseTree, compute_block_layout, compute_cached_layout,
};

use crate::{
    layout::{BoxKey, BoxLayoutTree, BoxNodeTy, inline::compute_inline_layout},
    node::UiTree,
};

pub(crate) struct TaffyLayoutImpl<'a>(pub &'a mut BoxLayoutTree, pub &'a mut UiTree);

impl LayoutPartialTree for TaffyLayoutImpl<'_> {
    type CoreContainerStyle<'a>
        = taffy::Style
    where
        Self: 'a;
    type CustomIdent = String;

    fn get_core_container_style(&self, node_id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        taffy::Style::DEFAULT
    }

    fn set_unrounded_layout(&mut self, node_id: taffy::NodeId, layout: &taffy::Layout) {
        self.0.map[from_taffy_key(node_id)].unrounded_layout = *layout;
    }

    fn compute_child_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        compute_cached_layout(self, node_id, inputs, |this, node_id, inputs| {
            let node = &mut this.0.map[from_taffy_key(node_id)];

            match node.ty {
                BoxNodeTy::Block(_) => compute_block_layout(this, node_id, inputs),
                BoxNodeTy::Inline(ref inline_box_item) => {
                    let items = inline_box_item.children.clone();
                    compute_inline_layout(this.1, this.0, &items)
                }
            }
        })
    }
}

impl TraversePartialTree for TaffyLayoutImpl<'_> {
    type ChildIter<'a>
        = ChildIter<'a>
    where
        Self: 'a;

    fn child_ids(&self, parent_node_id: taffy::NodeId) -> Self::ChildIter<'_> {
        let next_child_id = match self.0.map[from_taffy_key(parent_node_id)].ty {
            BoxNodeTy::Block(ref box_item) => box_item.first_child,
            BoxNodeTy::Inline(_) => None,
        };

        ChildIter {
            tree: self.0,
            next_child_id,
        }
    }

    fn child_count(&self, parent_node_id: taffy::NodeId) -> usize {
        match self.0.map[from_taffy_key(parent_node_id)].ty {
            BoxNodeTy::Block(ref box_item) => box_item.children_count,
            _ => 0,
        }
    }

    fn get_child_id(&self, parent_node_id: taffy::NodeId, child_index: usize) -> taffy::NodeId {
        // TODO:: impl workaround
        self.child_ids(parent_node_id).nth(child_index).unwrap()
    }
}

impl TraverseTree for TaffyLayoutImpl<'_> {}

impl RoundTree for TaffyLayoutImpl<'_> {
    fn get_unrounded_layout(&self, node_id: taffy::NodeId) -> taffy::Layout {
        self.0.map[from_taffy_key(node_id)].unrounded_layout
    }

    fn set_final_layout(&mut self, node_id: taffy::NodeId, layout: &taffy::Layout) {
        self.0.map[from_taffy_key(node_id)].layout = *layout;
    }
}

impl LayoutBlockContainer for TaffyLayoutImpl<'_> {
    type BlockContainerStyle<'a>
        = taffy::Style
    where
        Self: 'a;

    type BlockItemStyle<'a>
        = taffy::Style
    where
        Self: 'a;

    fn get_block_container_style(&self, node_id: taffy::NodeId) -> Self::BlockContainerStyle<'_> {
        taffy::Style::DEFAULT
    }

    fn get_block_child_style(&self, child_node_id: taffy::NodeId) -> Self::BlockItemStyle<'_> {
        taffy::Style::DEFAULT
    }
}

impl CacheTree for TaffyLayoutImpl<'_> {
    fn cache_get(
        &self,
        node_id: taffy::NodeId,
        known_dimensions: taffy::Size<Option<f32>>,
        available_space: taffy::Size<taffy::AvailableSpace>,
        run_mode: taffy::RunMode,
    ) -> Option<taffy::LayoutOutput> {
        self.0.map[from_taffy_key(node_id)]
            .cache
            .get(known_dimensions, available_space, run_mode)
    }

    fn cache_store(
        &mut self,
        node_id: taffy::NodeId,
        known_dimensions: taffy::Size<Option<f32>>,
        available_space: taffy::Size<taffy::AvailableSpace>,
        run_mode: taffy::RunMode,
        layout_output: taffy::LayoutOutput,
    ) {
        self.0.map[from_taffy_key(node_id)].cache.store(
            known_dimensions,
            available_space,
            run_mode,
            layout_output,
        );
    }

    fn cache_clear(&mut self, node_id: taffy::NodeId) {
        self.0.map[from_taffy_key(node_id)].cache.clear();
    }
}

pub fn from_taffy_key(id: taffy::NodeId) -> BoxKey {
    BoxKey(KeyData::from_ffi(id.into()))
}

pub fn to_taffy_key(id: BoxKey) -> taffy::NodeId {
    taffy::NodeId::new(id.0.as_ffi())
}

pub(crate) struct ChildIter<'a> {
    tree: &'a BoxLayoutTree,
    next_child_id: Option<BoxKey>,
}

impl Iterator for ChildIter<'_> {
    type Item = taffy::NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next_child_id.take()?;
        self.next_child_id = self.tree.map.get(id)?.next_sibiling;

        Some(to_taffy_key(id))
    }
}
