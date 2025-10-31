// use parley::{FontContext, InlineBox, LayoutContext, RangedBuilder};
// use taffy::compute_root_layout;

// use crate::{
//     layout::{
//         LayoutBoxKey, BoxLayoutTree, LayoutBox, LayoutTy, InlineItem,
//         taffy_impl::{TaffyLayoutImpl, to_taffy_key},
//     },
//     node::{NodeKey, UiTree},
// };

// pub fn compute_inline_layout(ui: &mut UiTree, box_tree: &mut BoxLayoutTree, id: LayoutBoxKey) {
//     // TODO:: move
//     let mut font_cx = FontContext::new();
//     let mut layout_cx = LayoutContext::<Option<NodeKey>>::new();

//     let Some(LayoutBox {
//         ty: LayoutTy::Inline(inline_box),
//         ..
//     }) = box_tree.boxes.get(id)
//     else {
//         return;
//     };

//     // todo:: remove clone
//     let text = inline_box.texts.clone();
//     let mut builder = layout_cx.ranged_builder(&mut font_cx, &text, 1.0, false);
//     traverse_inline(&mut builder, ui, box_tree, id);

//     // TODO:: cleanup code
//     let Some(LayoutBox {
//         ty: LayoutTy::Inline(inline_box),
//         ..
//     }) = box_tree.boxes.get_mut(id)
//     else {
//         return;
//     };
//     builder.build_into(&mut inline_box.parley_layout, &inline_box.texts);
// }

// pub fn traverse_inline(
//     builder: &mut RangedBuilder<Option<NodeKey>>,
//     ui: &mut UiTree,
//     box_tree: &mut BoxLayoutTree,
//     id: LayoutBoxKey,
// ) {
//     let inline_box = match box_tree.boxes.get_mut(id) {
//         Some(LayoutBox {
//             ty: LayoutTy::Inline(inline_box),
//             ..
//         }) => inline_box,

//         Some(LayoutBox {
//             ty: LayoutTy::Block(block_box),
//             ..
//         }) => {
//             compute_root_layout(
//                 &mut TaffyLayoutImpl(box_tree, ui),
//                 to_taffy_key(id),
//                 taffy::Size::min_content(),
//             );

//             let size = box_tree.boxes[id].taffy_layout.size;
//             builder.push_inline_box(InlineBox {
//                 id: 0,
//                 index: 0,
//                 width: size.width,
//                 height: size.height,
//             });

//             return;
//         }

//         _ => return,
//     };

//     // TODO:: remove clone
//     for &item in inline_box.children.clone().iter() {
//         match item {
//             InlineItem::Text { start, end } => {
//                 inline_box.texts.push_str(&box_tree.texts[start..end]);
//             }

//             InlineItem::Box(box_key) => {
//                 traverse_inline(builder, ui, box_tree, box_key);
//             }
//         }
//     }
// }
