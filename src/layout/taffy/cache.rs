use taffy::{AvailableSpace, CacheTree, LayoutOutput, NodeId, RunMode, Size};

use crate::layout::taffy::{TaffyLayoutImpl, from_taffy_key};

impl CacheTree for TaffyLayoutImpl<'_> {
    fn cache_get(
        &self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
    ) -> Option<LayoutOutput> {
        self.tree.boxes[from_taffy_key(node_id)].taffy_cache.get(
            known_dimensions,
            available_space,
            run_mode,
        )
    }

    fn cache_store(
        &mut self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
        layout_output: LayoutOutput,
    ) {
        self.tree.boxes[from_taffy_key(node_id)].taffy_cache.store(
            known_dimensions,
            available_space,
            run_mode,
            layout_output,
        );
    }

    fn cache_clear(&mut self, node_id: NodeId) {
        self.tree.boxes[from_taffy_key(node_id)].taffy_cache.clear();
    }
}
