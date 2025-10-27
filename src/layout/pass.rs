use core::iter;

use parley::{
    FontContext, InlineBox, Layout, LayoutContext, StyleProperty, TextStyle, TreeBuilder,
};

use crate::{
    layout::pre_pass::{LayoutPrePassCx, NodeIns},
    node::NodeKey,
};

pub struct LayoutPassCx {
    font_cx: FontContext,
    layout_cx: LayoutContext<Option<NodeKey>>,
    ins_stack_buf: Vec<NodeKey>,
    pub blocks: Vec<LayoutBlock>,
}

impl LayoutPassCx {
    pub fn new() -> Self {
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
            ins_stack_buf: Vec::new(),
            blocks: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.ins_stack_buf.clear();
        self.blocks.clear();
    }

    pub fn accept(&mut self, pre_pass: &mut LayoutPrePassCx) {
        self.clear();

        let mut builder: Option<TreeBuilder<'_, Option<NodeKey>>> = None;

        let mut text_size = 0usize;
        for ins in &pre_pass.instructions {
            match *ins {
                NodeIns::PushBlock(id, _style) => {
                    if let Some(builder) = builder.take() {
                        let (layout, text) = builder.build();
                        self.blocks.push(LayoutBlock { text, layout });
                    }

                    self.ins_stack_buf.push(id);
                    builder = Some(self.layout_cx.tree_builder(
                        &mut self.font_cx,
                        1.0,
                        false,
                        &TextStyle {
                            brush: Some(id),
                            ..Default::default()
                        },
                    ));
                }

                NodeIns::PushInline(id, _style) => {
                    // TODO:: error on invalid ins
                    builder
                        .as_mut()
                        .unwrap()
                        .push_style_modification_span(iter::once(&StyleProperty::Brush(Some(id))));
                    self.ins_stack_buf.push(id);
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
                    // TODO:: error on invalid ins
                    builder
                        .as_mut()
                        .unwrap()
                        .push_text(&pre_pass.texts[start..end]);
                    text_size += end - start;
                }

                NodeIns::PopInline => {
                    // TODO:: error on invalid ins
                    let Some(builder) = builder.as_mut() else {
                        continue;
                    };

                    builder.pop_style_span();
                }

                NodeIns::PopBlock => {
                    // TODO:: error on invalid ins
                    if let Some(builder) = builder.take() {
                        let (layout, text) = builder.build();
                        self.blocks.push(LayoutBlock { text, layout });
                    }

                    builder = Some(self.layout_cx.tree_builder(
                        &mut self.font_cx,
                        1.0,
                        false,
                        &TextStyle {
                            brush: self.ins_stack_buf.pop(),
                            ..Default::default()
                        },
                    ));

                    text_size = 0;
                }
            }
        }
    }
}

pub struct LayoutBlock {
    pub text: String,
    pub layout: Layout<Option<NodeKey>>,
}
