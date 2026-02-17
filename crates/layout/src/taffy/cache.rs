use taffy::{AvailableSpace, CacheTree, LayoutOutput, NodeId, RunMode, Size};

use crate::taffy::{TaffyLayout, from_taffy_key};

impl CacheTree for TaffyLayout<'_> {
    fn cache_get(
        &self,
        node_id: NodeId,
        known_dimensions: Size<Option<f32>>,
        available_space: Size<AvailableSpace>,
        run_mode: RunMode,
    ) -> Option<LayoutOutput> {
        self.tree.nodes[from_taffy_key(node_id)].cache.get(
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
        self.tree.nodes[from_taffy_key(node_id)].cache.store(
            known_dimensions,
            available_space,
            run_mode,
            layout_output,
        );
    }

    fn cache_clear(&mut self, node_id: NodeId) {
        self.tree.nodes[from_taffy_key(node_id)].cache.clear();
    }
}
