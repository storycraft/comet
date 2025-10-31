use parley::{Alignment, AlignmentOptions};
use slotmap::KeyData;
use taffy::{
    CacheTree, LayoutBlockContainer, LayoutPartialTree, TraversePartialTree, TraverseTree,
    compute_block_layout, compute_cached_layout, compute_leaf_layout,
};

use crate::{
    layout::{LayoutBox, LayoutBoxKey, LayoutBoxTree, LayoutTy, inline::compute_inline_layout},
    node::UiTree,
    tree::cursor::Cursor,
};

pub(crate) struct TaffyLayoutImpl<'a>(pub &'a mut LayoutBoxTree, pub &'a mut UiTree);

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
        self.0.boxes[from_taffy_key(node_id)].taffy_layout = *layout;
    }

    fn compute_child_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        compute_cached_layout(self, node_id, inputs, |this, node_id, inputs| {
            let id = from_taffy_key(node_id);
            let node = &mut this.0.boxes[id];

            match node.ty {
                LayoutTy::Block => compute_block_layout(this, node_id, inputs),
                LayoutTy::Inline(inline_box_key) => compute_leaf_layout(
                    inputs,
                    &taffy::Style::<String>::DEFAULT,
                    |_, _| 0.0,
                    |_, available_space| {
                        compute_inline_layout(this.1, this.0, inline_box_key);

                        let available_size = available_space.width.into_option();
                        let inline_box = &mut this.0.inline_boxes[inline_box_key];
                        inline_box.parley_layout.break_all_lines(available_size);
                        inline_box.parley_layout.align(
                            available_size,
                            Alignment::Start,
                            AlignmentOptions::default(),
                        );

                        taffy::Size {
                            width: inline_box.parley_layout.full_width(),
                            height: inline_box.parley_layout.height(),
                        }
                    },
                ),
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
        let first_id = self.0.boxes.first_child(from_taffy_key(parent_node_id));
        ChildIter {
            iter: self.0.boxes.cursor(first_id),
        }
    }

    fn child_count(&self, parent_node_id: taffy::NodeId) -> usize {
        self.0
            .boxes
            .cursor(self.0.boxes.first_child(from_taffy_key(parent_node_id)))
            .count()
    }

    fn get_child_id(&self, parent_node_id: taffy::NodeId, child_index: usize) -> taffy::NodeId {
        self.child_ids(parent_node_id).nth(child_index).unwrap()
    }
}

impl TraverseTree for TaffyLayoutImpl<'_> {}

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
        self.0.boxes[from_taffy_key(node_id)].taffy_cache.get(
            known_dimensions,
            available_space,
            run_mode,
        )
    }

    fn cache_store(
        &mut self,
        node_id: taffy::NodeId,
        known_dimensions: taffy::Size<Option<f32>>,
        available_space: taffy::Size<taffy::AvailableSpace>,
        run_mode: taffy::RunMode,
        layout_output: taffy::LayoutOutput,
    ) {
        self.0.boxes[from_taffy_key(node_id)].taffy_cache.store(
            known_dimensions,
            available_space,
            run_mode,
            layout_output,
        );
    }

    fn cache_clear(&mut self, node_id: taffy::NodeId) {
        self.0.boxes[from_taffy_key(node_id)].taffy_cache.clear();
    }
}

pub fn from_taffy_key(id: taffy::NodeId) -> LayoutBoxKey {
    LayoutBoxKey(KeyData::from_ffi(id.into()))
}

pub fn to_taffy_key(id: LayoutBoxKey) -> taffy::NodeId {
    taffy::NodeId::new(id.0.as_ffi())
}

pub(crate) struct ChildIter<'a> {
    iter: Cursor<'a, LayoutBoxKey, LayoutBox>,
}

impl Iterator for ChildIter<'_> {
    type Item = taffy::NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        Some(to_taffy_key(self.iter.next()?))
    }
}
