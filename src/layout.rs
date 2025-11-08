mod taffy;
pub mod tree;

use kurbo::{Point, Rect, Size};

pub type ContainerLayoutFn = fn();

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(transparent)]
pub struct ContainerLayout(ContainerLayoutFn);

impl ContainerLayout {
    pub const fn new(f: ContainerLayoutFn) -> Self {
        Self(f)
    }

    pub fn compute_layout(self) {
        // TODO
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayOuter {
    #[default]
    /// Element generates a block layout box.
    Block,
    /// Element is part of inline content.
    Inline,
}

#[derive(Debug, Default)]
pub enum DisplayInner {
    #[default]
    /// Generate layout boxes and display children using Flow layout.
    Flow,
    /// Establish a new flow context and layout children inside.
    FlowRoot,
    /// Layout each children with given layout algorithm.
    Container(ContainerLayout),
    /// Display a content inside. Children will not be laid out.
    Content,
}

impl PartialEq for DisplayInner {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Flow, Self::Flow) | (Self::FlowRoot, Self::FlowRoot) => true,
            (Self::Container(f1), Self::Container(f2)) => f1 == f2,
            _ => false,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Layout {
    /// Relative z-index of the layout box
    pub z_index: u32,

    /// Top-left location of the layout box
    pub location: Point,

    /// Bounding box size of the layout box
    pub size: Size,

    /// Rectangle representing the content area of the layout box.
    /// It can be larger than `size` when content overflows.
    pub content_size: Size,

    /// The size of the borders of the layout box
    pub border: Rect,

    /// The size of the padding of the layout box
    pub padding: Rect,

    /// The size of the margin of the layout box
    pub margin: Rect,
}

impl Layout {
    pub fn new() -> Self {
        Self {
            z_index: 0,
            location: Point::ORIGIN,
            size: Size::ZERO,
            content_size: Size::ZERO,
            border: Rect::ZERO,
            padding: Rect::ZERO,
            margin: Rect::ZERO,
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}
