use parley::{FontContext, InlineBox, LayoutContext, TextStyle, TreeBuilder};
use slotmap::Key;
use taffy::compute_root_layout;

use crate::{
    layout::{
        InlineBoxKey, InlineItem, InlineKey, LayoutTy,
        taffy::{TaffyLayoutImpl, to_taffy_key},
        tree::LayoutBoxTree,
    },
    ui::{Node, NodeKey, Ui},
};

pub fn compute_inline_layout(ui: &Ui, layout_tree: &mut LayoutBoxTree, id: InlineBoxKey) {
    // TODO:: move
    let mut font_cx = FontContext::new();
    let mut layout_cx = LayoutContext::<Option<NodeKey>>::new();

    // todo:: remove clone
    let mut builder = layout_cx.tree_builder(&mut font_cx, 1.0, false, &TextStyle::default());
    traverse_inline_box(&mut builder, ui, layout_tree, id);

    // TODO:: cleanup code
    let Some(inline_box) = layout_tree.inline_boxes.get_mut(id) else {
        return;
    };
    let (layout, texts) = builder.build();
    inline_box.parley_layout = layout;
    inline_box.texts = texts;
}

pub fn traverse_inline_box(
    builder: &mut TreeBuilder<Option<NodeKey>>,
    ui: &Ui,
    layout_box_tree: &mut LayoutBoxTree,
    id: InlineBoxKey,
) {
    let Some(inline_box) = layout_box_tree.inline_boxes.get(id) else {
        return;
    };

    let mut text_len = 0;
    let mut next_id = inline_box.item_start;
    while let Some(inline_id) = next_id {
        build_inline(builder, ui, layout_box_tree, inline_id, &mut text_len);
        next_id = layout_box_tree.inlines.next_sibling(inline_id);
    }
}

pub fn build_inline(
    builder: &mut TreeBuilder<Option<NodeKey>>,
    ui: &Ui,
    layout_box_tree: &mut LayoutBoxTree,
    id: InlineKey,
    text_len: &mut usize,
) {
    let Some(&inline_item) = layout_box_tree.inlines.get(id) else {
        return;
    };

    // TODO:: fix temp workaround
    match inline_item {
        InlineItem::Text(span) => {
            if let Some(Node::Text(text)) = ui.node(span).as_deref() {
                builder.push_text(text);
                *text_len += text.len();
            }
        }

        InlineItem::Box(layout_box_key) => match layout_box_tree.boxes[layout_box_key].ty {
            LayoutTy::Block => {
                compute_root_layout(
                    &mut TaffyLayoutImpl::new(layout_box_tree, ui),
                    to_taffy_key(layout_box_key),
                    taffy::Size::min_content(),
                );

                let size = layout_box_tree.boxes[layout_box_key].layout.size;
                builder.push_inline_box(InlineBox {
                    id: layout_box_key.data().as_ffi(),
                    index: *text_len,
                    width: size.width as _,
                    height: size.height as _,
                });
            }

            LayoutTy::Inline(inline_box_id) => {
                if let Some(inline_start) = layout_box_tree.inline_boxes[inline_box_id].item_start {
                    build_inline(builder, ui, layout_box_tree, inline_start, text_len);
                }
            }
        },
    }
}
