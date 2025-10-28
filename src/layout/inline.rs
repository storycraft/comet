use parley::{
    Alignment, AlignmentOptions, FontContext, InlineBox, LayoutContext, TextStyle, TreeBuilder,
};
use taffy::{CollapsibleMarginSet, compute_root_layout};

use crate::{
    layout::{
        BoxLayoutTree, InlineItem,
        taffy_impl::{TaffyLayoutImpl, to_taffy_key},
    },
    node::{Node, NodeKey, UiTree},
};

pub fn compute_inline_layout(
    ui: &mut UiTree,
    box_tree: &mut BoxLayoutTree,
    items: Vec<InlineItem>,
) -> taffy::LayoutOutput {
    // TODO:: move
    let mut font_cx = FontContext::new();
    let mut layout_cx = LayoutContext::<()>::new();

    let mut builder = layout_cx.tree_builder(&mut font_cx, 1.0, false, &TextStyle::default());
    let mut text_len = 0;
    for item in items {
        match item {
            InlineItem::Node(node_key) => {
                collect_texts(ui, node_key, &mut builder, &mut text_len);
            }

            InlineItem::Box(box_key) => {
                compute_root_layout(
                    &mut TaffyLayoutImpl(box_tree, ui),
                    to_taffy_key(box_key),
                    taffy::Size::min_content(),
                );

                let size = box_tree.map[box_key].layout.size;
                builder.push_inline_box(InlineBox {
                    id: 0,
                    index: text_len,
                    width: size.width,
                    height: size.height,
                });
            }
        }
    }

    let (mut layout, texts) = builder.build();
    layout.break_all_lines(None);
    layout.align(None, Alignment::Start, AlignmentOptions::default());
    let width = layout.width();
    let height = layout.height();

    taffy::LayoutOutput {
        size: taffy::Size { width, height },
        content_size: taffy::Size { width, height },
        first_baselines: taffy::Point::NONE,
        top_margin: CollapsibleMarginSet::ZERO,
        bottom_margin: CollapsibleMarginSet::ZERO,
        margins_can_collapse_through: false,
    }
}

fn collect_texts(ui: &UiTree, key: NodeKey, builder: &mut TreeBuilder<()>, text_len: &mut usize) {
    if let Some(Node::Text(text)) = ui.get(key) {
        builder.push_text(text);
        *text_len += text.len();
    }
}
