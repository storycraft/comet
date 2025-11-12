pub mod layout;
pub mod node;
pub mod renderer;
pub mod style;
pub mod tree;
pub mod store;

pub use kurbo;

use crate::{
    node::{Div, Node, NodeKey},
    tree::SlotTree,
};

#[non_exhaustive]
pub struct Ui {
    pub elements: SlotTree<NodeKey, Node>,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            elements: SlotTree::new(),
        }
    }

    /// Create a new Text node
    #[inline]
    pub fn create_text(&mut self, text: impl Into<String>) -> NodeKey {
        self.elements.insert(Node::Text(text.into()))
    }

    /// Create a new [`Div`] node
    #[inline]
    pub fn create_div(&mut self) -> NodeKey {
        self.elements.insert(Node::Div(Div::new()))
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}
