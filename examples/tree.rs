use std::{fs, io::BufWriter};

use anyrender::{ImageRenderer, Paint, PaintScene};
use anyrender_vello::VelloImageRenderer;
use color::AlphaColor;
use comet::{
    Ui,
    layout::{
        DisplayInner, DisplayOuter,
        tree::{InlineItem, LayoutBoxKey, LayoutBoxTree, LayoutBoxTreeCx, LayoutTy},
    },
    node::{Node, NodeKey},
    renderer::CometRenderer,
};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use kurbo::Affine;
use peniko::Brush;
use taffy::{LengthPercentageAuto, Rect};

fn main() {
    let mut ui = Ui::new();
    let root = ui.create_div();
    let text0 = ui.create_text("sample ");
    let text1 = ui.create_text(" text");
    let inner = ui.create_text("start");
    let div = ui.create_div();
    if let Some(Node::Div(div)) = ui.elements.get_mut(div) {
        div.display_outer = Some(DisplayOuter::Inline);
        div.display_inner = DisplayInner::FlowRoot;
    }

    let div2 = ui.create_div();
    if let Some(Node::Div(div2)) = ui.elements.get_mut(div2) {
        div2.margin = Rect {
            left: LengthPercentageAuto::length(50.0),
            top: LengthPercentageAuto::length(50.0),
            bottom: LengthPercentageAuto::length(50.0),
            right: LengthPercentageAuto::length(50.0),
        };
        div2.fill = Some(Paint::Solid(AlphaColor::from_rgb8(0, 255, 0)));
    }

    let inner2 = ui.create_text("end");
    let text2 = ui.create_text("1");
    ui.elements.append(root, text0);
    ui.elements.append(root, div);
    ui.elements.append(root, text1);
    ui.elements.append(div, inner);
    ui.elements.append(root, div2);
    ui.elements.append(div2, inner2);
    ui.elements.append(root, text2);

    let mut layout_tree = LayoutBoxTree::new();
    let mut tree_cx = LayoutBoxTreeCx::new();
    tree_cx.build(&ui, root, &mut layout_tree);
    let box_root = layout_tree.root;

    layout_tree.compute_layout(
        &mut ui,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(256.0),
            height: taffy::AvailableSpace::Definite(256.0),
        },
    );
    print_box_tree(&layout_tree, box_root, 0);

    print(&ui, root, 0);

    let mut vello_renderer = VelloImageRenderer::new(256, 256);
    let mut data = vec![0_u8; 256 * 256 * 4];
    vello_renderer.render(
        |scene| {
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                Brush::Solid(AlphaColor::WHITE),
                None,
                &kurbo::Rect::new(0.0, 0.0, 256.0, 256.0),
            );
            CometRenderer::new().draw(&ui, &layout_tree, box_root, scene);
        },
        &mut data[..],
    );

    PngEncoder::new(BufWriter::new(fs::File::create("render.png").unwrap()))
        .write_image(&data, 256, 256, ExtendedColorType::Rgba8)
        .unwrap();
}

fn print_box_tree(layout_tree: &LayoutBoxTree, id: LayoutBoxKey, space: u32) {
    let Some(node) = layout_tree.boxes.get(id) else {
        return;
    };

    for _ in 0..space {
        print!(" ");
    }
    println!(
        "- id: {id:?} span: {:?} ty: {:?} location: {:?} size: {:?}",
        node.span, node.ty, node.taffy_layout.location, node.taffy_layout.size
    );

    match node.ty {
        LayoutTy::Block => {}
        LayoutTy::Inline(inline_box_id) => {
            let inline_box = &layout_tree.inline_boxes[inline_box_id];
            for child_id in layout_tree.inlines.cursor(inline_box.item_start) {
                match layout_tree.inlines[child_id] {
                    InlineItem::Text { start, end } => {
                        for _ in 0..space {
                            print!(" ");
                        }
                        println!("    - text: {:?}", &layout_tree.texts[start..end]);
                    }
                    InlineItem::Box(child_box) => {
                        print_box_tree(layout_tree, child_box, space + 4);
                    }
                }
            }
        }
    }

    for child_id in layout_tree.boxes.cursor(layout_tree.boxes.first_child(id)) {
        print_box_tree(layout_tree, child_id, space + 4);
    }
}

fn print(tree: &Ui, id: NodeKey, space: u32) {
    for _ in 0..space {
        print!(" ");
    }
    println!("- {:?}", tree.elements.get(id));
    for child in tree.elements.cursor(tree.elements.first_child(id)) {
        print(tree, child, space + 4);
    }
}
