pub mod layout;
pub mod node;
pub mod renderer;
pub mod store;
pub mod style;
pub mod tree;

pub use kurbo;
pub use peniko;
pub use parley;

use crate::{
    node::{Node, NodeKey},
    store::PropStore,
    tree::SlotTree,
};

#[non_exhaustive]
pub struct Ui {
    pub elements: SlotTree<NodeKey, Node>,
    pub styles: PropStore,
}

impl Ui {
    pub fn new() -> Self {
        Self {
            elements: SlotTree::new(),
            styles: PropStore::new(),
        }
    }

    /// Create a new text node
    #[inline]
    pub fn create_text(&mut self, text: impl Into<String>) -> NodeKey {
        self.elements.insert(Node::Text(text.into()))
    }

    /// Create a new div node
    #[inline]
    pub fn create_div(&mut self) -> NodeKey {
        self.elements.insert(Node::Div)
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}
