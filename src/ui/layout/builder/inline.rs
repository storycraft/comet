use crate::{
    layout::tree::{
        LayoutTree,
        node::{InlineIns, InlineKey, InlineLayoutNode},
    },
    ui::NodeKey,
};

pub struct InlineStack {
    states: Vec<InlineState>,
    inline_mappings: Vec<NodeKey>,
}

impl InlineStack {
    pub fn new() -> Self {
        Self {
            states: vec![],
            inline_mappings: vec![],
        }
    }

    pub fn push(&mut self) {
        self.states.push(InlineState {
            node: InlineLayoutNode::new(None),
            last_inline: None,
            span_start: self.inline_mappings.len(),
        });
    }

    #[must_use]
    pub fn commit<'a>(
        &'a mut self,
    ) -> Option<(InlineLayoutNode, impl Iterator<Item = NodeKey> + 'a)> {
        let state = self.states.pop()?;
        let iter = self.inline_mappings.drain(state.span_start..);
        state.node.inline_start?;

        Some((state.node, iter))
    }

    pub fn add_ins(&mut self, tree: &mut LayoutTree, item: InlineIns) {
        if self.states.is_empty() {
            self.push();
        }
        let state = self.states.last_mut().unwrap();

        let key = tree.inlines.insert(item);
        if state.node.inline_start.is_none() {
            state.node.inline_start = Some(key);
        }
        if let Some(last_end) = state.last_inline.replace(key) {
            tree.inlines.after(last_end, key);
        }
    }

    pub fn add_text(&mut self, text: &str) {
        if self.states.is_empty() {
            self.push();
        }
        let state = self.states.last_mut().unwrap();
        state.node.texts.push_str(text);
    }

    pub fn add_span(&mut self, span: NodeKey) {
        if self.states.is_empty() {
            self.push();
        }
        self.inline_mappings.push(span);
    }
}

struct InlineState {
    node: InlineLayoutNode,
    last_inline: Option<InlineKey>,
    span_start: usize,
}
