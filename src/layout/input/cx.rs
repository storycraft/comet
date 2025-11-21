mod builder;

use rustc_hash::FxHashMap;

use crate::{
    layout::input::{
        InputNodeKey, InputNodeTy, LayoutInputTree, cx::builder::Builder, inline::InlineStack,
    },
    ui::{NodeKey, Ui},
};

pub struct LayoutInputTreeContext {
    parents: Vec<InputNodeKey>,
    inline: InlineStack,

    /// Mappings from [`NodeKey`] to [`InputNodeKey`] for invalidation
    mappings: FxHashMap<u32, InputNodeKey>,
}

impl LayoutInputTreeContext {
    pub fn new() -> Self {
        Self {
            parents: vec![],
            inline: InlineStack::new(),
            mappings: FxHashMap::default(),
        }
    }

    fn invalidate_inner(&mut self, tree: &mut LayoutInputTree, key: InputNodeKey) {
        let Some(node) = tree.nodes.get_mut(key) else {
            return;
        };
        if node.cache.is_empty() {
            return;
        }

        node.cache.clear();
        let parent = tree.nodes.parent(key);
        if let Some(parent) = parent {
            self.invalidate_inner(tree, parent);
        }
    }

    pub fn invalidate(
        &mut self,
        tree: &mut LayoutInputTree,
        node: NodeKey,
    ) -> Option<InputNodeKey> {
        let target_node_key = *self.mappings.get(&node.id())?;
        self.invalidate_inner(tree, target_node_key);
        Some(target_node_key)
    }

    pub fn update(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        node: NodeKey,
    ) -> Option<InputNodeKey> {
        let mut target_node_key = self.mappings.get(&node.id()).copied()?;
        // Find nearest spanned block parent
        let target_span = loop {
            let node = tree.nodes.get(target_node_key)?;
            if let InputNodeTy::Block(Some(span)) = node.ty {
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

        let Some(first_child) = ui.first_child(target_span) else {
            return None;
        };
        self.build(ui, first_child, tree, target_node_key);

        Some(target_node_key)
    }

    pub fn build(
        &mut self,
        ui: &Ui,
        root_node: NodeKey,
        tree: &mut LayoutInputTree,
        root_input_node: InputNodeKey,
    ) {
        Builder { cx: self, ui, tree }.build(root_node, root_input_node);
    }

    pub fn layout(&mut self, tree: &mut LayoutInputTree, root: InputNodeKey) {
        
    }
}

impl Default for LayoutInputTreeContext {
    fn default() -> Self {
        Self::new()
    }
}
