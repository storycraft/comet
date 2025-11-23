mod inner;

use rustc_hash::FxBuildHasher;
use slotmap::SparseSecondaryMap;

use crate::layout::{
    inline::{
        builder::inner::{Builder, InlineState},
        tree::{InlineNodeKey, InlineTree},
    },
    tree::{InlineLayoutNodeKey, LayoutTree},
};

/// Build [`InlineTree`] and keep synced with [`LayoutTree`]
pub struct InlineTreeBuilder {
    parents: Vec<InlineNodeKey>,
    states: Vec<InlineState>,

    /// Mappings from [`InlineLayoutNodeKey`] to [`InlineNodeKey`] for invalidation
    mappings: SparseSecondaryMap<InlineLayoutNodeKey, InlineNodeKey, FxBuildHasher>,
}

impl InlineTreeBuilder {
    pub fn new() -> Self {
        Self {
            parents: vec![],
            states: vec![],
            mappings: SparseSecondaryMap::default(),
        }
    }

    #[inline]
    pub fn get(&self, key: InlineLayoutNodeKey) -> Option<InlineNodeKey> {
        self.mappings.get(key).copied()
    }

    /// Invalidate inline tree built with layout_node.
    /// Old tree is deleted.
    pub fn invalidate(
        &mut self,
        tree: &mut InlineTree,
        layout_node: InlineLayoutNodeKey,
    ) -> Option<InlineNodeKey> {
        let line_start_key = self.mappings.remove(layout_node)?;
        tree.delete_node(line_start_key);
        Some(line_start_key)
    }

    /// Invalidate old inline tree and rebuild inline tree
    pub fn build(
        &mut self,
        layout_tree: &LayoutTree,
        tree: &mut InlineTree,
        layout_node: InlineLayoutNodeKey,
    ) -> Option<InlineNodeKey> {
        self.invalidate(tree, layout_node);
        Builder {
            cx: self,
            layout_tree,
            tree,
            next_height: 0.0,
        }
        .build(layout_node)
    }
}

impl Default for InlineTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}
