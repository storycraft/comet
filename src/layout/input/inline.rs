use crate::{
    layout::input::{InlineIns, InlineKey, InlineNode, LayoutInputTree},
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
            node: InlineNode::new(None),
            last_inline: None,
            span_start: self.inline_mappings.len(),
        });
    }

    #[must_use]
    pub fn commit<'a>(&'a mut self) -> Option<(InlineNode, impl Iterator<Item = NodeKey> + 'a)> {
        let state = self.states.pop()?;
        let iter = self.inline_mappings.drain(state.span_start..);
        state.node.inline_start?;

        Some((state.node, iter))
    }

    pub fn add_ins(&mut self, tree: &mut LayoutInputTree, item: InlineIns) {
        let Some(state) = self.states.last_mut() else {
            return;
        };

        let key = tree.inlines.insert(item);
        if state.node.inline_start.is_none() {
            state.node.inline_start = Some(key);
        }
        if let Some(last_end) = state.last_inline.replace(key) {
            tree.inlines.after(last_end, key);
        }
    }

    pub fn add_text(&mut self, text: &str) {
        let Some(state) = self.states.last_mut() else {
            return;
        };
        state.node.texts.push_str(text);
    }

    pub fn add_span(&mut self, span: NodeKey) {
        if self.states.is_empty() {
            return;
        }
        self.inline_mappings.push(span);
    }
}

struct InlineState {
    node: InlineNode,
    last_inline: Option<InlineKey>,
    span_start: usize,
}
