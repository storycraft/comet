use parley::FontContext;

use crate::layout::{
    fragment::{LayoutBoxTree, builder::LayoutBoxTreeBuilderContext},
    input::{InlineIns, InputNode, InputNodeKey, InputNodeTy, LayoutInputTree},
};

pub struct InlineBuilder<'a> {
    pub cx: &'a mut LayoutBoxTreeBuilderContext,
    pub font_cx: &'a mut FontContext,
    pub input_tree: &'a LayoutInputTree,
    pub tree: &'a mut LayoutBoxTree,
}

impl<'a> InlineBuilder<'a> {
    pub fn build(mut self, key: InputNodeKey) -> Option<()> {
        self.compute_inline_boxes(key);
        // TODO:: reuse layout
        let mut layout = parley::Layout::<()>::new();
        self.layout(key, &mut layout);

        Some(())
    }

    fn layout(&mut self, key: InputNodeKey, layout: &mut parley::Layout<()>) {
        let Some(InputNode {
            ty: InputNodeTy::Inline(inline_node),
            ..
        }) = self.input_tree.nodes.get(key)
        else {
            return;
        };

        let builder =
            self.cx
                .inline_layout
                .ranged_builder(self.font_cx, &inline_node.texts, 1.0, false);

        let mut current_index = 0usize;
        for child in self.input_tree.inlines.cursor(inline_node.inline_start) {
            let Some(inline_ins) = self.input_tree.inlines.get(child) else {
                break;
            };

            match inline_ins {
                InlineIns::Text(length) => {
                    current_index += length;
                }

                InlineIns::PushInlineBox(node_key) => {
                    // TODO
                }

                InlineIns::PopInlineBox => {}

                InlineIns::Node(input_node_key) => {}
            }
        }

        builder.build_into(layout, &inline_node.texts);
    }

    fn compute_inline_boxes(&mut self, key: InputNodeKey) {
        let Some(InputNode {
            ty: InputNodeTy::Inline(inline_node),
            ..
        }) = self.input_tree.nodes.get(key)
        else {
            return;
        };

        let mut next_id = inline_node.inline_start;
        while let Some(inline_id) = next_id {
            next_id = self.input_tree.inlines.next_sibling(inline_id);
            let Some(InlineIns::Node(input_node_key)) = self.input_tree.inlines.get(inline_id)
            else {
                continue;
            };

            // self.cx.layout(
            //     self.font_cx,
            //     self.ui,
            //     self.tree,
            //     *layout_box_key,
            //     taffy::Size::min_content(),
            // );
        }
    }
}
