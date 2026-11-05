use super::{ChildLayoutMode, Frame, LayoutItemTy, LayoutTree, NodeKey, item, same_shape};

/// Tracks nested formatting contexts and the inline spans resumed after a block.
#[derive(Default)]
pub struct Flows {
    stack: Vec<Flow>,
    spans: Vec<NodeKey>,
}

impl Flows {
    pub fn push(&mut self, tree: &mut LayoutTree, frames: &mut [Frame]) {
        if let Some(frame) = frames.len().checked_sub(1) {
            let key = frames[frame].key;

            // When block flow becomes inline flow, unwrap its old inline runs.
            if tree.items[key].child_layout == ChildLayoutMode::Inline {
                let mut next = tree.items.first_child(key);

                while let Some(child) = next {
                    next = tree.items.next_sibling(child);

                    if same_shape(
                        tree,
                        child,
                        item(LayoutItemTy::AnonymousBlock, ChildLayoutMode::Inline),
                    ) {
                        while let Some(content) = tree.items.first_child(child) {
                            tree.items.before(child, content);
                        }

                        tree.delete_item(child);
                    }
                }

                frames[frame].next = tree.items.first_child(key);
            }

            self.stack.push(Flow {
                frame,
                span_start: self.spans.len(),
                inline_run: false,
            });
        }
    }

    pub fn pop(&mut self) -> Option<usize> {
        let flow = self.stack.pop()?;
        self.spans.truncate(flow.span_start);

        Some(flow.frame)
    }

    pub fn current_frame(&self) -> Option<usize> {
        Some(self.stack.last()?.frame)
    }

    pub fn set_inline_run(&mut self, active: bool) -> Option<()> {
        self.stack.last_mut()?.inline_run = active;

        Some(())
    }

    pub fn needs_inline_run(&self, tree: &LayoutTree, frames: &[Frame]) -> Option<bool> {
        let flow = self.stack.last()?;

        Some(
            frames.len() == flow.frame + 1
                && tree.items[frames[flow.frame].key].child_layout == ChildLayoutMode::Block,
        )
    }

    /// The direct parent of an anonymous inline run, while that run is current.
    pub fn inline_parent(&self, frame_count: usize) -> Option<usize> {
        let flow = self.stack.last()?;

        (flow.inline_run && frame_count == flow.frame + 2).then_some(flow.frame)
    }

    pub fn resume_span_start(&self, frame_count: usize) -> Option<usize> {
        let flow = self.stack.last()?;
        let base = flow.frame + 1 + usize::from(flow.inline_run);

        Some(flow.span_start + frame_count.checked_sub(base)?)
    }

    pub fn push_span(&mut self, node: NodeKey) {
        self.spans.push(node);
    }

    pub fn pop_span(&mut self) -> Option<NodeKey> {
        self.spans.pop()
    }

    pub fn span(&self, index: usize) -> Option<NodeKey> {
        self.spans.get(index).copied()
    }

    pub fn clear(&mut self) {
        self.stack.clear();
        self.spans.clear();
    }
}

struct Flow {
    frame: usize,
    span_start: usize,
    inline_run: bool,
}
