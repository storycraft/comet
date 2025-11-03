pub mod container;
mod taffy;
pub mod tree;

pub type LayoutFn = fn();

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(transparent)]
pub struct Layout(LayoutFn);

impl Layout {
    pub const fn new(f: LayoutFn) -> Self {
        Self(f)
    }

    pub fn compute_layout(self) {
        // TODO
    }
}

pub trait GenericLayout: 'static + Sized + Clone {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayOuter {
    #[default]
    Block,
    Inline,
}

#[derive(Debug, Default)]
pub enum DisplayInner {
    #[default]
    Flow,
    FlowRoot,
    Layout(Layout),
    Content,
}

impl PartialEq for DisplayInner {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Flow, Self::Flow) | (Self::FlowRoot, Self::FlowRoot) => true,
            (Self::Layout(f1), Self::Layout(f2)) => f1 == f2,
            _ => false,
        }
    }
}
