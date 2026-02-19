use comet_div::ui::Ui;
use parley::{FontContext, InlineBox};
use slotmap::Key;

use crate::{
    cx::LayoutContext,
    parley::default_text_style,
    resolve::text::resolve_text_style,
    tree::{InlineIns, InlineLayoutNodeKey, LayoutTree},
};

pub struct InlineLayout<'a> {
    font_cx: &'a mut FontContext,
    cx: &'a mut LayoutContext,
    ui: &'a mut Ui,
    tree: &'a mut LayoutTree,
    text_len: usize,
}

impl<'a> InlineLayout<'a> {
    pub fn new(
        font_cx: &'a mut FontContext,
        cx: &'a mut LayoutContext,
        ui: &'a mut Ui,
        tree: &'a mut LayoutTree,
    ) -> Self {
        Self {
            font_cx,
            cx,
            ui,
            tree,
            text_len: 0,
        }
    }

    pub fn compute_layout(mut self, id: InlineLayoutNodeKey) {
        self.compute_inline_nodes(id);
        let Some(inline_node) = self.tree.inline_nodes.get_mut(id) else {
            return;
        };

        let text = std::mem::take(&mut inline_node.texts);
        let mut builder =
            self.cx
                .parley
                .tree_builder(self.font_cx, 1.0, false, &default_text_style(()));

        let mut next_id = inline_node.inline_start;
        while let Some(inline_id) = next_id {
            next_id = self.tree.inlines.next_sibling(inline_id);
            let Some(&inline_item) = self.tree.inlines.get(inline_id) else {
                continue;
            };

            match inline_item {
                InlineIns::Text(length) => {
                    builder.push_text(&text[self.text_len..][..length]);
                    self.text_len += length;
                }

                InlineIns::PushInlineBox(node) => {
                    builder.push_style_span(resolve_text_style(self.ui, node));
                }

                InlineIns::PopInlineBox => {
                    builder.pop_style_span();
                }

                InlineIns::Node(inline_node_key) => {
                    let size = self.tree.nodes[inline_node_key].layout.size;
                    builder.push_inline_box(InlineBox {
                        id: inline_node_key.data().as_ffi(),
                        index: self.text_len,
                        width: size.width as _,
                        height: size.height as _,
                    });
                }
            }
        }

        let Some(inline_node) = self.tree.inline_nodes.get_mut(id) else {
            return;
        };
        builder.build_into(&mut inline_node.layout);
        inline_node.texts = text;
    }

    fn compute_inline_nodes(&mut self, id: InlineLayoutNodeKey) {
        let Some(inline_node) = self.tree.inline_nodes.get(id) else {
            return;
        };

        let mut next_id = inline_node.inline_start;
        while let Some(inline_id) = next_id {
            next_id = self.tree.inlines.next_sibling(inline_id);

            let Some(InlineIns::Node(inline_node_key)) = self.tree.inlines.get(inline_id) else {
                continue;
            };

            self.cx.layout(
                self.font_cx,
                self.ui,
                self.tree,
                *inline_node_key,
                taffy::Size::min_content(),
            );
        }
    }
}
