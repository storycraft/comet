mod common;

use std::{fs, io::BufWriter};

use anyrender::{ImageRenderer, PaintScene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use comet_layout::{
    fragment2::{FragmentTree, FragmentTreeBuilder},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{LayoutConstraint, geometry::Size},
};
use comet_paint::CometPainter;
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use kurbo::Affine;

const WIDTH: u32 = 640;
const HEIGHT: u32 = 920;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (ui, root) = common::sample_ui();
    let viewport = Size::new(WIDTH as f64, HEIGHT as f64);
    let mut inputs = LayoutInputTree::new();
    let input_root = LayoutInputTreeBuilder::new()
        .build(&ui, &mut inputs, BuildRequest::new(root, 16.0, viewport))
        .ok_or("could not build layout input tree")?;
    let mut fragments = FragmentTree::new();
    let constraint = LayoutConstraint::new_root(viewport, 16.0);
    let root = FragmentTreeBuilder::new()
        .build(&ui, &inputs, input_root, &mut fragments, &constraint)
        .ok_or("could not build fragment tree")?;
    inputs.clear_flags(input_root);

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

    let mut renderer = CometPainter::new();
    let mut image_renderer = VelloCpuImageRenderer::new(WIDTH, HEIGHT);
    let mut data = vec![0_u8; (WIDTH * HEIGHT * 4) as usize];
    image_renderer.render(
        |scene| {
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                common::BACKGROUND,
                None,
                &kurbo::Rect::new(0.0, 0.0, WIDTH as f64, HEIGHT as f64),
            );
            renderer.draw(&ui, &inputs, &fragments, root, scene);
        },
        &mut data,
    );

    PngEncoder::new(BufWriter::new(fs::File::create("render.png")?)).write_image(
        &data,
        WIDTH,
        HEIGHT,
        ExtendedColorType::Rgba8,
    )?;

    Ok(())
}
