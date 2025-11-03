pub mod tree;
pub mod container;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayOuter {
    #[default]
    Block,
    Inline,
}

#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq)]
pub enum DisplayInner {
    #[default]
    Flow,
    FlowRoot,
    Layout(Layout),
}
