mod common;

use std::{fs, io::BufWriter};

use anyrender::{ImageRenderer, PaintScene};
use anyrender_vello::VelloImageRenderer;
use comet_layout2::{
    fragment::{builder::FragmentBuilder, tree::FragmentTree},
    style::{LayoutConstraint, geometry::Size},
    tree::{LayoutTree, builder::LayoutTreeBuilder},
};
use comet_renderer::CometRenderer;
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use kurbo::Affine;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (ui, root) = common::sample_ui();
    let mut layout = LayoutTree::new();
    let root = LayoutTreeBuilder::new()
        .build(&ui, root, &mut layout, None)
        .ok_or("could not build layout tree")?;
    let mut fragments = FragmentTree::new();
    let constraint = LayoutConstraint::new_root(Size::new(256.0, 256.0), 16.0);
    let root = FragmentBuilder::new()
        .build(&constraint, &ui, &layout, root, &mut fragments)
        .ok_or("could not build fragment tree")?;

    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let fragment = &fragments.items[key];
        println!(
            "{}{key:?} {:?} {:?}",
            "  ".repeat(depth),
            fragment.item,
            fragment.layout
        );

        pending.extend(
            fragments
                .items
                .children(key)
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }

    let mut renderer = CometRenderer::new();
    let mut image_renderer = VelloImageRenderer::new(256, 256);
    let mut data = vec![0_u8; 256 * 256 * 4];
    image_renderer.render(
        |scene| {
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                peniko::Color::WHITE,
                None,
                &kurbo::Rect::new(0.0, 0.0, 256.0, 256.0),
            );
            renderer.draw(&ui, &fragments, root, scene);
        },
        &mut data,
    );

    PngEncoder::new(BufWriter::new(fs::File::create("render.png")?)).write_image(
        &data,
        256,
        256,
        ExtendedColorType::Rgba8,
    )?;

    Ok(())
}
