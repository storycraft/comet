use slotmap::new_key_type;
use taffy::{BoxSizing, Dimension, LengthPercentage, LengthPercentageAuto, Position, Rect, Size};

use crate::{
    layout::{DisplayInner, DisplayOuter},
    tree::SlotTree,
};

#[derive(Debug, PartialEq)]
pub enum Node {
    Div(Div),
    Text(String),
}

#[derive(Debug, PartialEq)]
pub struct Div {
    // Display
    pub display_outer: Option<DisplayOuter>,
    pub display_inner: DisplayInner,

    // Position
    pub position: Position,

    // Size and modes
    pub box_sizing: BoxSizing,
    pub size: Size<Dimension>,
    pub min_size: Size<Dimension>,
    pub max_size: Size<Dimension>,

    // Margin, padding, border
    pub margin: Rect<LengthPercentageAuto>,
    pub padding: Rect<LengthPercentage>,
    pub border: Rect<LengthPercentage>,
}

impl Div {
    const fn new() -> Self {
        Self {
            display_outer: Some(DisplayOuter::Block),
            display_inner: DisplayInner::Flow,
            position: Position::Relative,
            box_sizing: BoxSizing::BorderBox,
            size: Size::auto(),
            min_size: Size::auto(),
            max_size: Size::auto(),
            margin: Rect::zero(),
            padding: Rect::zero(),
            border: Rect::zero(),
        }
    }
}

impl Default for Div {
    fn default() -> Self {
        Self::new()
    }
}

new_key_type! { pub struct NodeKey; }

pub struct UiTree {
    pub elements: SlotTree<NodeKey, Node>,
}

impl Default for UiTree {
    fn default() -> Self {
        Self::new()
    }
}

impl UiTree {
    pub fn new() -> Self {
        Self {
            elements: SlotTree::new(),
        }
    }

    /// Create a new Text node
    pub fn create_text(&mut self, text: impl Into<String>) -> NodeKey {
        self.elements.insert(Node::Text(text.into()))
    }

    /// Create a new [`Div`] node
    pub fn create_div(&mut self) -> NodeKey {
        self.elements.insert(Node::Div(Div::new()))
    }
}
