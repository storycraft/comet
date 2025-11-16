use parley::{FontContext, InlineBox, TextStyle, TreeBuilder};
use slotmap::Key;

use crate::{
    layout::{
        InlineBoxKey, InlineIns, InlineKey,
        taffy::{LayoutContext, TaffyLayoutImpl},
        tree::LayoutBoxTree,
    },
    ui::{Node, NodeKey, Ui},
};

pub struct InlineLayout<'a> {
    builder: TreeBuilder<'a, NodeKey>,
    ui: &'a Ui,
    tree: &'a mut LayoutBoxTree,
    text_len: usize,
}

impl<'a> InlineLayout<'a> {
    pub fn new(font_cx: &'a mut FontContext, layout: &'a mut TaffyLayoutImpl) -> Self {
        Self {
            builder: layout
                .cx
                .parley
                .tree_builder(font_cx, 1.0, false, &TextStyle::default()),
            ui: layout.ui,
            tree: layout.tree,
            text_len: 0,
        }
    }

    pub fn compute_layout(mut self, id: InlineBoxKey) {
        self.traverse_inline_box(id);

        let Some(inline_box) = self.tree.inline_boxes.get_mut(id) else {
            return;
        };
        let (layout, texts) = self.builder.build();
        inline_box.parley_layout = layout;
        inline_box.texts = texts;
    }

    fn traverse_inline_box(&mut self, id: InlineBoxKey) {
        let Some(inline_box) = self.tree.inline_boxes.get(id) else {
            return;
        };

        let mut next_id = inline_box.inline_start;
        while let Some(inline_id) = next_id {
            self.build_inline(inline_id);
            next_id = self.tree.inlines.next_sibling(inline_id);
        }
    }

    fn build_inline(&mut self, id: InlineKey) {
        let Some(&inline_item) = self.tree.inlines.get(id) else {
            return;
        };

        // TODO:: fix temp workaround
        match inline_item {
            InlineIns::Text(span) => {
                if let Some(Node::Text(text)) = self.ui.node(span).as_deref() {
                    self.builder.push_text(text);
                    self.text_len += text.len();
                }
            }

            InlineIns::PushInlineBox(_) => {}
            InlineIns::PopInlineBox => {}

            InlineIns::Box(layout_box_key) => {
                // TODO:: remove temporary layout context
                LayoutContext::new().layout(
                    self.ui,
                    self.tree,
                    layout_box_key,
                    taffy::Size::min_content(),
                );

                let size = self.tree.boxes[layout_box_key].layout.size;
                self.builder.push_inline_box(InlineBox {
                    id: layout_box_key.data().as_ffi(),
                    index: self.text_len,
                    width: size.width as _,
                    height: size.height as _,
                });
            }
        }
    }
}
