//! Paint benchmark: the 100,000 panels of `layout_bench`, each with a fill and
//! the text "test", painted with `CometPainter` into a 1280x720 viewport.
//!
//! Most panels lie below the viewport. To see how cheaply they are discarded,
//! every case is compared with a tree holding only the panels that are
//! visible, which is the cost of painting with perfect culling.
//!
//! Run with `cargo run --release -p comet --example paint_bench`.

use std::time::{Duration, Instant};

use anyrender::{ImageRenderer, Paint, Scene, recording::RenderCommand};
use anyrender_vello_cpu::VelloCpuImageRenderer;
use comet_layout::{
    dirty_map::DirtyMap,
    fragment::{builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint,
        bx::{DisplayInner, Size as BoxSize},
        geometry::{AutoValue, Size, StyleUnit},
    },
};
use comet_layout_flex::{flex, styles as flex_style};
use comet_paint::{
    layer::{PaintLayerKey, builder::PaintLayerBuilder, tree::PaintLayerTree},
    painter::CometPainter,
    pre_paint::{builder::PrePaintTreeBuilder, tree::PrePaintTree},
    style::Fill,
};
use comet_tree::{
    node::{Node, Text},
    ui::Ui,
};
use peniko::Color;

const PANELS: usize = 100_000;
const ROOT_FONT_SIZE: f64 = 16.0;
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const SCALE_FACTOR: f64 = 1.0;
const RUNS: usize = 20;

#[derive(Clone, Copy, PartialEq)]
enum Sizing {
    Fixed,
    Auto,
}

/// A laid out tree, ready to paint.
struct Laid {
    ui: Ui,
    inputs: LayoutInputTree,
    fragments: FragmentTree,
    pre_paint: PrePaintTree,
    layers: PaintLayerTree,
    layer_root: PaintLayerKey,
}

fn build(sizing: Sizing, panels: usize) -> Laid {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let container = ui.create(
        Node::Box,
        (
            DisplayInner::Container(flex()),
            flex_style::FlexWrap(flex_style::taffy::FlexWrap::Wrap),
        ),
    );
    ui.append(root, container);

    for _ in 0..panels {
        let panel = ui.create(
            Node::Box,
            (Fill(Paint::Solid(Color::from_rgb8(224, 233, 255))),),
        );
        if sizing == Sizing::Fixed {
            ui.set_props(
                panel,
                (BoxSize(Size::new(
                    AutoValue::Value(StyleUnit::Px(48.0)),
                    AutoValue::Value(StyleUnit::Px(24.0)),
                )),),
            );
        }
        let text = ui.create(Node::Text, (Text("test".into()),));
        ui.append(panel, text);
        ui.append(container, panel);
    }

    let viewport = Size::new(
        WIDTH as f64 / SCALE_FACTOR,
        HEIGHT as f64 / SCALE_FACTOR,
    );
    let mut inputs = LayoutInputTree::new();
    let mut dirty = DirtyMap::new();
    let input_root = LayoutInputTreeBuilder::new()
        .build(
            &ui,
            &mut inputs,
            &mut dirty,
            BuildRequest::new(root, ROOT_FONT_SIZE, viewport),
        )
        .expect("prelayout");
    let constraint = LayoutConstraint::new_root(viewport, ROOT_FONT_SIZE);
    let mut fragments = FragmentTree::new();
    let fragment_root = FragmentTreeBuilder::new()
        .build(
            &ui,
            &inputs,
            &mut dirty,
            input_root,
            &mut fragments,
            &constraint,
        )
        .expect("layout");
    let mut pre_paint = PrePaintTree::new();
    PrePaintTreeBuilder::new()
        .build(
            &ui,
            &inputs,
            &mut dirty,
            &fragments,
            &constraint.units,
            &mut pre_paint,
            fragment_root,
            false,
        )
        .expect("pre-paint");
    let mut layers = PaintLayerTree::new();
    let layer_root = PaintLayerBuilder::new()
        .build(
            &fragments,
            &inputs,
            &dirty,
            &pre_paint,
            &mut layers,
            input_root,
        )
        .expect("layerize");
    dirty.clear(&inputs, input_root);

    Laid {
        ui,
        inputs,
        fragments,
        pre_paint,
        layers,
        layer_root,
    }
}

/// Paint commands emitted by one paint.
#[derive(Default, PartialEq, Debug)]
struct Commands {
    fills: usize,
    glyph_runs: usize,
    other: usize,
}

struct Bench {
    painter: CometPainter,
    scene: Scene,
    renderer: VelloCpuImageRenderer,
    buffer: Vec<u8>,
}

impl Bench {
    fn new() -> Self {
        Self {
            painter: CometPainter::new(),
            scene: Scene::new(),
            renderer: VelloCpuImageRenderer::new(WIDTH, HEIGHT),
            buffer: vec![0; (WIDTH * HEIGHT * 4) as usize],
        }
    }

    /// Paint into a recording scene: the painter's own cost, without
    /// rasterizing. Returns the median time and the commands emitted.
    fn record(&mut self, laid: &Laid, viewport: kurbo::Rect) -> (Duration, Commands) {
        let times = (0..RUNS)
            .map(|_| {
                self.scene.commands.clear();
                let start = Instant::now();
                self.painter.draw(
                    &laid.ui,
                    &laid.inputs,
                    &laid.fragments,
                    &laid.pre_paint,
                    &laid.layers,
                    laid.layer_root,
                    viewport,
                    SCALE_FACTOR,
                    &mut self.scene,
                );
                start.elapsed()
            })
            .collect();

        let mut commands = Commands::default();
        for command in &self.scene.commands {
            match command {
                RenderCommand::Fill(_) => commands.fills += 1,
                RenderCommand::GlyphRun(_) => commands.glyph_runs += 1,
                _ => commands.other += 1,
            }
        }

        (median(times), commands)
    }

    /// Paint and rasterize with the CPU image renderer. Returns the median
    /// time spent painting inside the render callback and the median total.
    fn render(&mut self, laid: &Laid, viewport: kurbo::Rect) -> (Duration, Duration) {
        let mut paints = Vec::with_capacity(RUNS);
        let mut totals = Vec::with_capacity(RUNS);

        for _ in 0..RUNS {
            let painter = &mut self.painter;
            let mut paint = Duration::ZERO;
            let start = Instant::now();
            // The renderer keeps commands until reset, so every frame starts
            // with one, as a window renderer does.
            self.renderer.reset();
            self.renderer.render(
                |scene| {
                    let start = Instant::now();
                    painter.draw(
                        &laid.ui,
                        &laid.inputs,
                        &laid.fragments,
                        &laid.pre_paint,
                        &laid.layers,
                        laid.layer_root,
                        viewport,
                        SCALE_FACTOR,
                        scene,
                    );
                    paint = start.elapsed();
                },
                &mut self.buffer,
            );
            totals.push(start.elapsed());
            paints.push(paint);
        }

        (median(paints), median(totals))
    }
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

fn ms(d: Duration) -> String {
    format!("{:>9.3}", d.as_secs_f64() * 1000.0)
}

fn row(case: &str, laid: &Laid, bench: &mut Bench, viewport: kurbo::Rect) -> Commands {
    let (record, commands) = bench.record(laid, viewport);
    let (paint, total) = bench.render(laid, viewport);
    println!(
        "| {case:<26} | {:>9} | {:>6} | {:>6} | {} | {} | {} | {} |",
        laid.fragments.items.len(),
        commands.fills,
        commands.glyph_runs,
        ms(record),
        ms(paint),
        ms(total - paint),
        ms(total),
    );
    commands
}

fn scenario(sizing: Sizing, bench: &mut Bench) {
    let name = match sizing {
        Sizing::Fixed => "Fixed-size panels",
        Sizing::Auto => "Auto-size panels",
    };
    println!("\n### {name} ({WIDTH}x{HEIGHT}, median ms)\n");
    println!(
        "| {:<26} | {:>9} | {:>6} | {:>6} | {:>9} | {:>9} | {:>9} | {:>9} |",
        "Case", "Fragments", "Fills", "Glyphs", "Record", "Paint", "Raster", "Total"
    );
    println!(
        "|{:-<28}|{:-<11}|{:-<8}|{:-<8}|{:-<11}|{:-<11}|{:-<11}|{:-<11}|",
        "", "", "", "", "", "", "", ""
    );

    let screen = kurbo::Rect::new(0.0, 0.0, WIDTH as f64, HEIGHT as f64);
    let all = build(sizing, PANELS);
    let commands = row(&format!("{PANELS} panels"), &all, bench, screen);

    // Every visible panel paints one fill, so the fills count the visible
    // panels. Panels wrap in order, so the first ones are the visible ones.
    let visible = build(sizing, commands.fills);
    let culled = row(
        &format!("{} panels (visible only)", commands.fills),
        &visible,
        bench,
        screen,
    );
    assert_eq!(commands.fills, culled.fills, "the visible panels paint the same boxes");

    // A viewport far below the content: every fragment is culled, so this is
    // the cost of walking the tree alone.
    let below = screen + kurbo::Vec2::new(0.0, 1.0e9);
    row(&format!("{PANELS} panels, all culled"), &all, bench, below);
}

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("warning: debug build; run with --release for meaningful numbers");
    }
    let mut bench = Bench::new();
    empty_frame(&mut bench, "before painting");
    scenario(Sizing::Fixed, &mut bench);
    scenario(Sizing::Auto, &mut bench);
    empty_frame(&mut bench, "after painting");
}

/// Rasterize a frame with nothing painted: the renderer's fixed cost.
fn empty_frame(bench: &mut Bench, when: &str) {
    let times = (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            // The renderer keeps commands until reset, like a new frame.
            bench.renderer.reset();
            bench.renderer.render(|_| {}, &mut bench.buffer);
            start.elapsed()
        })
        .collect();
    println!("\nEmpty frame raster ({when}): {} ms", ms(median(times)).trim());
}

