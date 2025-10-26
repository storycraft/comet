use parley::{FontContext, InlineBox, Layout, LayoutContext, TextStyle, TreeBuilder};

use crate::node::{DisplayOuter, LayoutTree, NodeKey};

#[derive(Debug, Clone, Copy)]
pub enum NodeIns {
    /// Push inline/block box
    Push(DisplayOuter, Option<()>),
    /// An opaque box
    Box { width: f32, height: f32 },
    /// A text span
    Text { start: usize, end: usize },
    /// Pop previously pushed box
    Pop,
}

#[derive(Debug)]
pub struct LayoutPrePass {
    ins_buf: Vec<NodeIns>,
    text_buf: String,
}

impl LayoutPrePass {
    pub fn new() -> Self {
        Self {
            ins_buf: Vec::new(),
            text_buf: String::new(),
        }
    }

    pub fn clear(&mut self) {
        self.ins_buf.clear();
        self.text_buf.clear();
    }
}

pub fn layout_pre_pass(pass: &mut LayoutPrePass, tree: &LayoutTree, root: NodeKey) {
    fn pre_pass_inner(tree: &LayoutTree, pass: &mut LayoutPrePass, id: NodeKey) {
        let Some(node) = tree.get(id) else {
            return;
        };

        match node {
            crate::node::Node::Div(div) => {
                let display_outer = div.display.unwrap_or_default().0;
                pass.ins_buf.push(NodeIns::Push(display_outer, None));

                for &child in tree.children(id) {
                    pre_pass_inner(tree, pass, child);
                }
                pass.ins_buf.push(NodeIns::Pop);
            }

            crate::node::Node::Text(text) => {
                let start = pass.text_buf.len();
                pass.text_buf.push_str(text);
                pass.ins_buf.push(NodeIns::Text {
                    start,
                    end: start + text.len(),
                });
            }
        }
    }

    pass.clear();
    pre_pass_inner(tree, pass, root);
}

pub struct LayoutPass {
    font_cx: FontContext,
    layout_cx: LayoutContext<()>,
    pub blocks: Vec<LayoutBlock>,
}

impl LayoutPass {
    pub fn new() -> Self {
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
            blocks: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.blocks.clear();
    }
}

pub struct LayoutBlock {
    pub text: String,
    pub layout: Layout<()>,
}

pub fn layout(pre_pass: &mut LayoutPrePass, pass: &mut LayoutPass) {
    let mut builder: Option<TreeBuilder<'_, ()>> = None;

    let mut tmp_buf = Vec::<DisplayOuter>::new();
    let mut text_size = 0usize;
    for ins in &pre_pass.ins_buf {
        match *ins {
            NodeIns::Push(display_outer, _style) => {
                tmp_buf.push(display_outer);
                if let Some(builder) = builder.take() {
                    let (layout, text) = builder.build();
                    pass.blocks.push(LayoutBlock { text, layout });
                }

                builder = Some(pass.layout_cx.tree_builder(
                    &mut pass.font_cx,
                    1.0,
                    true,
                    &TextStyle::default(),
                ));
            }

            NodeIns::Box { width, height } => {
                builder.as_mut().unwrap().push_inline_box(InlineBox {
                    id: 0, // TODO
                    index: text_size,
                    width,
                    height,
                });
            }

            NodeIns::Text { start, end } => {
                builder
                    .as_mut()
                    .unwrap()
                    .push_text(&pre_pass.text_buf[start..end]);
                text_size += end - start;
            }

            NodeIns::Pop => {
                if let Some(DisplayOuter::Block) = tmp_buf.pop() {
                    if let Some(builder) = builder.take() {
                        let (layout, text) = builder.build();
                        pass.blocks.push(LayoutBlock { text, layout });
                    }

                    builder = Some(pass.layout_cx.tree_builder(
                        &mut pass.font_cx,
                        1.0,
                        true,
                        &TextStyle::default(),
                    ));

                    text_size = 0;
                }
            }
        }
    }
}
