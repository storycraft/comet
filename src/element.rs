#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Element {
    Div(Div),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Div {
    pub display: Option<(DisplayOuter, DisplayInner)>,
}

impl Div {
    pub const fn new() -> Self {
        Self {
            display: Some((DisplayOuter::Block, DisplayInner::Flow)),
        }
    }
}

impl Default for Div {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayOuter {
    Block,
    Inline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayInner {
    Flow,
    FlowRoot,
    Flex,
    Grid,
}
