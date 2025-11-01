use rustc_hash::FxBuildHasher;
use slotmap::{Key, SecondaryMap, SlotMap, SparseSecondaryMap};

use crate::node::{NodeKey, UiTree};

/// Styles having inheritable properties.
pub trait ResolvableStyle {
    /// Resolved style type
    type Resolved: Clone;

    /// Resolve styles using parent styles
    fn resolve(&self, parent: &Self::Resolved) -> Self::Resolved;
}

pub struct StyleTree<K: Key, Style: ResolvableStyle> {
    pub root_style: Style::Resolved,
    pub styles: SlotMap<K, Style>,
    pub node_styles: SparseSecondaryMap<NodeKey, Style, FxBuildHasher>,
    pub resolved_node_styles: SecondaryMap<NodeKey, Style::Resolved>,
}

impl<K: Key, Style: ResolvableStyle> StyleTree<K, Style> {
    /// Create new [`StyleTree`] with root style
    pub fn new(root_style: Style::Resolved) -> Self {
        Self {
            root_style,
            styles: SlotMap::with_key(),
            node_styles: SparseSecondaryMap::with_hasher(FxBuildHasher),
            resolved_node_styles: SecondaryMap::new(),
        }
    }

    /// calculate resolved style for a node
    pub fn resolve(&mut self, tree: &UiTree, node: NodeKey) {
        if self.resolved_node_styles.get(node).is_some() {
            return;
        }

        let resolved_style = if let Some(parent_id) = tree.parent(node) {
            self.resolve(tree, parent_id);
            self.resolved_node_styles.get(parent_id)
        } else {
            None
        };

        match self.node_styles.get(node) {
            Some(style) => {
                self.resolved_node_styles.insert(
                    node,
                    style.resolve(resolved_style.unwrap_or(&self.root_style)),
                );
            }
            None => {
                self.resolved_node_styles
                    .insert(node, self.root_style.clone());
            }
        }
    }

    /// Invalidate resolved styles for a node and its children
    pub fn invalidate(&mut self, tree: &UiTree, node: NodeKey) {
        if self.resolved_node_styles.remove(node).is_none() {
            return;
        }

        for &child_id in tree.children(node) {
            self.invalidate(tree, child_id);
        }
    }
}
