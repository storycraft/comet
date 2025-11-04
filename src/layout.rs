mod taffy;
pub mod tree;

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
    Block,
    Inline,
}

#[derive(Debug, Default)]
pub enum DisplayInner {
    #[default]
    Flow,
    FlowRoot,
    Container(ContainerLayout),
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
