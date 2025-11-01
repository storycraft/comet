pub mod tree;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayOuter {
    #[default]
    Block,
    Inline,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayInner {
    #[default]
    Flow,
    FlowRoot,
    Flex,
    Grid,
}
