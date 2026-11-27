//! Render the sample UI into `render.png` with the CPU renderer, through a
//! [`Window`] without a platform window.

mod common;

use std::{fs, io::BufWriter};

use anyrender::{AntiAliasing, ImageRenderer, PaintScene};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use comet::{shell::window::Window, winit::dpi::PhysicalSize};
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};
use kurbo::Affine;

const WIDTH: u32 = 1100;
const HEIGHT: u32 = 2600;
/// Physical pixels per logical unit of the rendered image.
const SCALE_FACTOR: f64 = 1.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut window = Window::new(common::app);
    window.resize(PhysicalSize::new(WIDTH, HEIGHT), SCALE_FACTOR);

    if !window.layout() {
        return Err("could not lay out the UI".into());
    }

    let mut renderer = VelloCpuImageRenderer::new(WIDTH, HEIGHT);
    let mut data = vec![0_u8; (WIDTH * HEIGHT * 4) as usize];
    renderer.render(
        |scene| {
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                common::BACKGROUND,
                None,
                &kurbo::Rect::new(0.0, 0.0, WIDTH as f64, HEIGHT as f64),
                AntiAliasing::Enabled,
            );
            window.paint(scene);
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
