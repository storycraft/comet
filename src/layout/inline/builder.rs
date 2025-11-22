#[derive(Debug)]
pub struct InlineTreeBuilder {}

impl InlineTreeBuilder {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for InlineTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}
