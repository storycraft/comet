use parley::{
    Alignment, AlignmentOptions, Brush, FontContext, InlineBox, LayoutContext, TextStyle,
    TreeBuilder,
};
use taffy::{CollapsibleMarginSet, compute_root_layout};

use crate::{
    layout::{
        BoxLayoutTree, InlineItem, TreeNodeTy,
        taffy_impl::{TaffyLayoutImpl, to_taffy_key},
    },
    node::{Node, NodeKey, UiTree},
};

pub fn compute_inline_layout(
    ui: &mut UiTree,
    box_tree: &mut BoxLayoutTree,
    items: &[InlineItem],
) -> taffy::LayoutOutput {
    // TODO:: move
    let mut font_cx = FontContext::new();
    let mut layout_cx = LayoutContext::<Option<NodeKey>>::new();

    let mut builder = layout_cx.tree_builder(&mut font_cx, 1.0, false, &TextStyle::default());
    traverse_inline(&mut builder, ui, box_tree, items, &mut 0);

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

pub fn traverse_inline(
    builder: &mut TreeBuilder<Option<NodeKey>>,
    ui: &mut UiTree,
    box_tree: &mut BoxLayoutTree,
    items: &[InlineItem],
    text_len: &mut usize,
) {
    for &item in items {
        match item {
            InlineItem::Text(node_key) => {
                collect_texts(ui, node_key, builder, text_len);
            }

            InlineItem::Box(box_key) => match box_tree.map[box_key].ty {
                TreeNodeTy::Box(_) => {
                    compute_root_layout(
                        &mut TaffyLayoutImpl(box_tree, ui),
                        to_taffy_key(box_key),
                        taffy::Size::min_content(),
                    );

                    let size = box_tree.map[box_key].layout.size;
                    builder.push_inline_box(InlineBox {
                        id: 0,
                        index: *text_len,
                        width: size.width,
                        height: size.height,
                    });
                }

                TreeNodeTy::Inline(ref item) => {
                    traverse_inline(builder, ui, box_tree, &item.children.clone(), text_len);
                }
            },
        }
    }
}

fn collect_texts<B: Brush>(
    ui: &UiTree,
    key: NodeKey,
    builder: &mut TreeBuilder<B>,
    text_len: &mut usize,
) {
    if let Some(Node::Text(text)) = ui.get(key) {
        builder.push_text(text);
        *text_len += text.len();
    }
}
