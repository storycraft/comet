mod common;

use std::{fs, io::BufWriter};

use anyrender::{AntiAliasing, ImageRenderer, PaintScene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use comet_layout::{
    dirty_map::DirtyMap,
    fragment::{builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{LayoutConstraint, geometry::Size},
};
use comet_paint::{
    layer::{builder::PaintLayerBuilder, tree::PaintLayerTree},
    painter::CometPainter,
    prepaint::{builder::PrepaintTreeBuilder, tree::PrepaintTree},
};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use kurbo::Affine;

const WIDTH: u32 = 1100;
const HEIGHT: u32 = 1400;
/// Physical pixels per logical unit of the rendered image.
const SCALE_FACTOR: f64 = 1.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sample = comet::UiRoot::new(common::sample_ui);
    let (tree, root) = (sample.tree().borrow(), sample.root());
    let ui = &tree.ui;
    // Layout works in logical units.
    let viewport = Size::new(WIDTH as f64 / SCALE_FACTOR, HEIGHT as f64 / SCALE_FACTOR);
    let mut inputs = LayoutInputTree::new();
    let mut dirty = DirtyMap::new();
    let input_root = LayoutInputTreeBuilder::new()
        .build(
            ui,
            &mut inputs,
            &mut dirty,
            BuildRequest::new(root, 16.0, viewport),
        )
        .ok_or("could not build layout input tree")?;
    let mut fragments = FragmentTree::new();
    let constraint = LayoutConstraint::new_root(viewport, 16.0);
    let root = FragmentTreeBuilder::new()
        .build(
            ui,
            &inputs,
            &mut dirty,
            input_root,
            &mut fragments,
            &constraint,
        )
        .ok_or("could not build fragment tree")?;
    let mut prepaint = PrepaintTree::new();
    PrepaintTreeBuilder::new()
        .build(
            ui,
            &inputs,
            &mut dirty,
            &fragments,
            &constraint.units,
            &mut prepaint,
            root,
            false,
        )
        .ok_or("could not resolve pre-paint tree")?;
    let mut layers = PaintLayerTree::new();
    let layer_root = PaintLayerBuilder::new()
        .build(
            &fragments,
            &inputs,
            &dirty,
            &prepaint,
            &mut layers,
            input_root,
        )
        .ok_or("could not build paint layer tree")?;
    dirty.clear(&inputs, input_root);

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
                AntiAliasing::Enabled,
            );
            renderer.draw(
                ui,
                &inputs,
                &fragments,
                &prepaint,
                &layers,
                layer_root,
                kurbo::Rect::new(0.0, 0.0, WIDTH as f64, HEIGHT as f64),
                SCALE_FACTOR,
                scene,
            );
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
