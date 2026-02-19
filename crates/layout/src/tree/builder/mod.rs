mod inline;
mod inner;

use comet_div::ui::{NodeKey, Ui};
use rustc_hash::FxHashMap;

use crate::tree::{
    LayoutTree,
    builder::{inline::InlineStack, inner::Builder},
    {LayoutNodeKey, LayoutNodeTy},
};

/// Incremental [`LayoutTree`] builder
pub struct UiLayoutBuilder {
    parents: Vec<LayoutNodeKey>,
    inline: InlineStack,

    /// Mappings from [`NodeKey`] to [`LayoutNodeKey`] for invalidation
    mappings: FxHashMap<u32, LayoutNodeKey>,
}

impl UiLayoutBuilder {
    pub fn new() -> Self {
        Self {
            parents: vec![],
            inline: InlineStack::new(),
            mappings: FxHashMap::default(),
        }
    }

    pub fn get(&self, node: NodeKey) -> Option<LayoutNodeKey> {
        self.mappings.get(&node.id()).cloned()
    }

    /// Invalidate resolved layout of [`NodeKey`].
    /// Returns affected [`LayoutNodeKey`].
    pub fn invalidate_layout(
        &mut self,
        tree: &mut LayoutTree,
        node: NodeKey,
    ) -> Option<LayoutNodeKey> {
        let target_node_key = *self.mappings.get(&node.id())?;
        tree.invalidate(target_node_key);
        Some(target_node_key)
    }

    /// Try rebuilding [`LayoutTree`] incrementally within [`NodeKey`].
    /// Returns affected parent [`LayoutNodeKey`].
    pub fn update(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutTree,
        node: NodeKey,
    ) -> Option<LayoutNodeKey> {
        let mut target_node_key = self.mappings.get(&node.id()).copied()?;
        // Find nearest spanned block parent
        let target_span = loop {
            let node = tree.nodes.get(target_node_key)?;
            if let LayoutNodeTy::Block(Some(span)) = node.ty {
                break span;
            }

            target_node_key = tree.nodes.parent(target_node_key)?;
        };

        // Clear all children
        let mut next_child = tree.nodes.first_child(target_node_key);
        while let Some(child) = next_child {
            next_child = tree.nodes.next_sibling(child);
            tree.delete_node(child);
        }

        // Perform rebuild
        self.build(ui, ui.first_child(target_span)?, tree, target_node_key);
        Some(target_node_key)
    }

    pub fn build(
        &mut self,
        ui: &Ui,
        root_node: NodeKey,
        tree: &mut LayoutTree,
        root_input_node: LayoutNodeKey,
    ) {
        Builder { cx: self, ui, tree }.build(root_node, root_input_node);
    }
}

impl Default for UiLayoutBuilder {
    fn default() -> Self {
        Self::new()
    }
}
