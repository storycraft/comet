use crate::node::{DisplayInner, DisplayOuter, UiTree, Node, NodeKey};

#[derive(Debug, Clone, Copy)]
pub enum NodeIns {
    /// Push block box
    PushBlock(NodeKey, Option<()>),
    /// Push inline box
    PushInline(NodeKey, Option<()>),
    /// An opaque box
    Box {
        key: NodeKey,
        width: f32,
        height: f32,
    },
    /// A text span
    Text { start: usize, end: usize },
    /// Pop previouslyt pushed inline box
    PopInline,
    /// Pop previously pushed block box
    PopBlock,
}

#[derive(Debug)]
pub struct LayoutPrePassCx {
    pub instructions: Vec<NodeIns>,
    pub texts: String,
}

impl LayoutPrePassCx {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            texts: String::new(),
        }
    }

    pub fn clear(&mut self) {
        self.instructions.clear();
        self.texts.clear();
    }

    pub fn accept(&mut self, tree: &UiTree, root: NodeKey) {
        self.clear();
        self.pre_pass_inner(tree, root);
    }

    fn pre_pass_inner(&mut self, tree: &UiTree, id: NodeKey) {
        let Some(node) = tree.get(id) else {
            return;
        };

        match node {
            Node::Div(div) => {
                let (display_outer, display_inner) = div.display.unwrap_or_default();

                let (push_ins, pop_ins) = match display_outer {
                    DisplayOuter::Block => (NodeIns::PushBlock(id, None), NodeIns::PopBlock),
                    DisplayOuter::Inline => (NodeIns::PushInline(id, None), NodeIns::PopInline),
                };

                self.instructions.push(push_ins);
                if display_inner == DisplayInner::Flow {
                    for &child in tree.children(id) {
                        self.pre_pass_inner(tree, child);
                    }
                } else {
                    self.instructions.push(NodeIns::Box {
                        key: id,
                        // TODO
                        width: 100.0,
                        height: 100.0,
                    });
                }

                self.instructions.push(pop_ins);
            }

            Node::Text(text) => {
                let start = self.texts.len();
                self.texts.push_str(text);
                self.instructions.push(NodeIns::Text {
                    start,
                    end: start + text.len(),
                });
            }
        }
    }
}
