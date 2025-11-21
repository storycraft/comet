
use parley::{FontContext, InlineBox};
use slotmap::Key;

use crate::{
    layout::{InlineBoxKey, InlineIns, taffy::LayoutContext, tree::LayoutBoxTree},
    ui::Ui,
};

pub struct InlineLayout<'a> {
    font_cx: &'a mut FontContext,
    cx: &'a mut LayoutContext,
    ui: &'a Ui,
    tree: &'a mut LayoutBoxTree,
    text_len: usize,
}

impl<'a> InlineLayout<'a> {
    pub fn new(
        font_cx: &'a mut FontContext,
        cx: &'a mut LayoutContext,
        ui: &'a Ui,
        tree: &'a mut LayoutBoxTree,
    ) -> Self {
        Self {
            font_cx,
            cx,
            ui,
            tree,
            text_len: 0,
        }
    }

    pub fn compute_layout(mut self, id: InlineBoxKey) {
        self.compute_inline_boxes(id);
        let Some(inline_box) = self.tree.inline_boxes.get_mut(id) else {
            return;
        };

        let text = std::mem::take(&mut inline_box.texts);
        let mut builder = self
            .cx
            .parley
            .ranged_builder(self.font_cx, &text, 1.0, false);
        let mut next_id = inline_box.inline_start;

        while let Some(inline_id) = next_id {
            next_id = self.tree.inlines.next_sibling(inline_id);
            let Some(&inline_item) = self.tree.inlines.get(inline_id) else {
                continue;
            };

            match inline_item {
                InlineIns::Text(length) => {
                    self.text_len += length;
                }

                InlineIns::PushInlineBox(_) => {}
                InlineIns::PopInlineBox => {}

                InlineIns::Box(layout_box_key) => {
                    let size = self.tree.boxes[layout_box_key].layout.size;
                    builder.push_inline_box(InlineBox {
                        id: layout_box_key.data().as_ffi(),
                        index: self.text_len,
                        width: size.width as _,
                        height: size.height as _,
                    });
                }
            }
        }

        let Some(inline_box) = self.tree.inline_boxes.get_mut(id) else {
            return;
        };
        builder.build_into(&mut inline_box.parley_layout, &text);
        inline_box.texts = text;
    }

    fn compute_inline_boxes(&mut self, id: InlineBoxKey) {
        let Some(inline_box) = self.tree.inline_boxes.get_mut(id) else {
            return;
        };
        let mut next_id = inline_box.inline_start;

        while let Some(inline_id) = next_id {
            next_id = self.tree.inlines.next_sibling(inline_id);

            let Some(InlineIns::Box(layout_box_key)) = self.tree.inlines.get(inline_id) else {
                continue;
            };

            self.cx.layout(
                self.font_cx,
                self.ui,
                self.tree,
                *layout_box_key,
                taffy::Size::min_content(),
            );
        }
    }
}
