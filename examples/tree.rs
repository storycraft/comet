use std::{fs, io::BufWriter};

use anyrender::{ImageRenderer, Paint, PaintScene};
use anyrender_vello::VelloImageRenderer;
use color::AlphaColor;
use comet::{
    layout::{
        cx::LayoutContext,
        tree::{LayoutTree, node::{LayoutNodeKey, LayoutNodeTy}},
    },
    renderer::CometRenderer,
    style::div::{DisplayInner, DisplayOuter, Fill, Padding},
    ui::{Node, NodeKey, Ui, layout::UiLayoutBuilder},
};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use kurbo::Affine;
use parley::FontContext;
use peniko::Brush;
use taffy::{LengthPercentage, Rect};

fn main() {
    let mut ui = Ui::new();
    let root = ui.create_node(Node::Div, ());
    let text0 = ui.create_node(Node::Text("sample".to_string()), ());
    let text1 = ui.create_node(Node::Text("text".to_string()), ());
    let inner = ui.create_node(Node::Text("start".to_string()), ());
    let div = ui.create_node(
        Node::Div,
        (
            DisplayOuter::Inline,
            Fill(Paint::Solid(AlphaColor::from_rgb8(255, 0, 0))),
        ),
    );
    let div1 = ui.create_node(
        Node::Div,
        (
            DisplayOuter::Inline,
            DisplayInner::FlowRoot,
            Fill(Paint::Solid(AlphaColor::from_rgb8(255, 255, 0))),
        ),
    );

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

    let mut layout_tree = LayoutTree::new();
    let layout_root = layout_tree.create_root();

    let mut tree_builder = UiLayoutBuilder::new();
    tree_builder.build(&ui, root, &mut layout_tree, layout_root);

    let mut layout_cx = LayoutContext::new();
    layout_cx.layout(
        &mut FontContext::new(),
        &ui,
        &mut layout_tree,
        layout_root,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(256.0),
            height: taffy::AvailableSpace::Definite(256.0),
        },
    );

    print_layout_tree(&ui, &layout_tree, layout_root, 0);

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

fn print_layout_tree(ui: &Ui, layout_tree: &LayoutTree, id: LayoutNodeKey, space: u32) {
    let Some(node) = layout_tree.nodes.get(id) else {
        return;
    };

    for _ in 0..space {
        print!(" ");
    }
    println!(
        "- id: {id:?} ty: {:?} location: {:?} size: {:?}",
        node.ty, node.layout.location, node.layout.size
    );

    match node.ty {
        LayoutNodeTy::Block(_) => {}
        LayoutNodeTy::Inline(inline_node_id) => {
            let inline_box = &layout_tree.inline_nodes[inline_node_id];
            for _ in 0..(space + 4) {
                print!(" ");
            }

            for child_id in layout_tree.inlines.cursor(inline_box.inline_start) {
                print!("{:?}, ", layout_tree.inlines[child_id]);
            }
            println!();
        }
    }

    for child_id in layout_tree.nodes.cursor(layout_tree.nodes.first_child(id)) {
        print_layout_tree(ui, layout_tree, child_id, space + 4);
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
