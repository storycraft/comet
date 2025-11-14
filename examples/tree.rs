use std::{fs, io::BufWriter};

use anyrender::{ImageRenderer, Paint, PaintScene};
use anyrender_vello::VelloImageRenderer;
use color::AlphaColor;
use comet::{
    layout::{InlineItem, LayoutBoxKey, LayoutTy, tree::{LayoutBoxTree, builder::LayoutTreeBuilder}},
    renderer::CometRenderer,
    style::{
        StyleRect, StyleUnit,
        div::{BorderRadius, DisplayInner, DisplayOuter, Fill, Padding},
    },
    ui::{Node, NodeKey, Ui},
};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use kurbo::Affine;
use peniko::Brush;
use taffy::{LengthPercentage, Rect};

fn main() {
    let mut ui = Ui::new();
    let root = ui.create_node(Node::Div, ());
    let text0 = ui.create_node(Node::Text("sample ".to_string()), ());
    let text1 = ui.create_node(Node::Text(" text".to_string()), ());
    let inner = ui.create_node(Node::Text("start".to_string()), ());
    let div = ui.create_node(
        Node::Div,
        (
            DisplayOuter::Inline,
            Fill(Paint::Solid(AlphaColor::from_rgb8(255, 0, 0))),
        ),
    );
    let div1 = ui.create_node(Node::Div, (DisplayOuter::Inline, DisplayInner::FlowRoot));

    let div2 = ui.create_node(
        Node::Div,
        (
            Padding(Rect {
                left: LengthPercentage::length(16.0),
                top: LengthPercentage::length(16.0),
                bottom: LengthPercentage::length(16.0),
                right: LengthPercentage::length(16.0),
            }),
            Fill(Paint::Solid(AlphaColor::from_rgb8(0, 255, 0))),
            BorderRadius(StyleRect {
                top: StyleUnit::Px(8.0),
                right: StyleUnit::ZERO,
                bottom: StyleUnit::Px(8.0),
                left: StyleUnit::ZERO,
            }),
        ),
    );

    let inner2 = ui.create_node(Node::Text("end".to_string()), ());
    let text2 = ui.create_node(Node::Text("1".to_string()), ());
    ui.append(div, text0);
    ui.append(div, div1);
    ui.append(div, text1);
    ui.append(root, div);
    ui.append(div1, inner);
    ui.append(root, div2);
    ui.append(div2, inner2);
    ui.append(root, text2);

    let mut layout_tree = LayoutBoxTree::new();
    let layout_root = layout_tree.create_root_box();
    let mut tree_builder = LayoutTreeBuilder::new();
    tree_builder.build_children(&ui, &mut layout_tree, root, layout_root);

    layout_tree.compute_layout(
        &mut ui,
        layout_root,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(256.0),
            height: taffy::AvailableSpace::Definite(256.0),
        },
    );
    print_box_tree(&ui, &layout_tree, layout_root, 0);

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
            CometRenderer::new().draw(&ui, &layout_tree, layout_root, scene);
        },
        &mut data[..],
    );

    PngEncoder::new(BufWriter::new(fs::File::create("render.png").unwrap()))
        .write_image(&data, 256, 256, ExtendedColorType::Rgba8)
        .unwrap();
}

fn print_box_tree(ui: &Ui, layout_tree: &LayoutBoxTree, id: LayoutBoxKey, space: u32) {
    let Some(node) = layout_tree.boxes.get(id) else {
        return;
    };

    for _ in 0..space {
        print!(" ");
    }
    println!(
        "- id: {id:?} span: {:?} ty: {:?} location: {:?} size: {:?}",
        node.span, node.ty, node.layout.location, node.layout.size
    );

    match node.ty {
        LayoutTy::Block => {}
        LayoutTy::Inline(inline_box_id) => {
            let inline_box = &layout_tree.inline_boxes[inline_box_id];
            for child_id in layout_tree.inlines.cursor(inline_box.item_start) {
                match layout_tree.inlines[child_id] {
                    InlineItem::Text(span) => {
                        if let Some(Node::Text(text)) = ui.node(span).as_deref() {
                            for _ in 0..space {
                                print!(" ");
                            }

                            println!("    - text: {:?}", &text);
                        }
                    }
                    InlineItem::Box(child_box) => {
                        print_box_tree(ui, layout_tree, child_box, space + 4);
                    }
                }
            }
        }
    }

    for child_id in layout_tree.boxes.cursor(layout_tree.boxes.first_child(id)) {
        print_box_tree(ui, layout_tree, child_id, space + 4);
    }
}

fn print(tree: &Ui, id: NodeKey, space: u32) {
    for _ in 0..space {
        print!(" ");
    }
    println!("- {id:?} {:?}", tree.node(id));
    for child in tree.cursor(tree.first_child(id)) {
        print(tree, child, space + 4);
    }
}
