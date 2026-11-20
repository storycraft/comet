//! A reactive counter in a window: click the button to count up. A rounded
//! square spins on the right, driven by a frame handler.

use core::f64::consts::TAU;

use anyrender::Paint;
use comet::{prelude::*, winit::window::WindowAttributes};
use comet_layout::style::{parley, taffy::Position};
use comet_layout_flex::styles::taffy::{AlignItems, FlexDirection, JustifyContent};
use comet_tree::node::NodeKey;
use kurbo::Affine;
use peniko::Color;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    comet::run(WindowAttributes::default().with_title("comet counter"), app)
}

fn app(cx: &UiCx) -> NodeKey {
    // A flex root shrinks to its content without a width; fill the window so
    // the halves below have a width to take.
    cx.div()
        .role(Role::Main)
        .flex()
        .size(size(vw(100), auto()))
        .min_size(size(auto(), vh(100)))
        .fill(Paint::Solid(Color::from_rgb8(239, 242, 247)))
        .text_color(Paint::Solid(Color::from_rgb8(36, 46, 63)))
        .children(|cx| {
            counter(cx);
            spinner(cx);
        })
}

fn counter(cx: &UiCx) -> NodeKey {
    let (count, set_count) = signal(0);

    cx.div()
        .size(size(percent(50), auto()))
        .flex()
        .flex_direction(FlexDirection::Column)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .gap(size(px(16), px(16)))
        .children(|cx| {
            // Text changes lay out again.
            cx.text_with(move || format!("Clicked {} times", count.get()))
                .font_size(px(28))
                .build();
            button(cx, "Count up", move || {
                set_count.update(|count| *count += 1)
            });
        })
}

fn button(cx: &UiCx, label: &str, on_click: impl Fn() + 'static) -> NodeKey {
    let (hovered, set_hovered) = signal(false);

    cx.div()
        .role(Role::Button)
        .padding(axis(px(10), px(18)))
        .border_radius(all(px(8)))
        .text_color(Paint::Solid(Color::WHITE))
        .font_weight(parley::FontWeight::SEMI_BOLD)
        // Fill changes only repaint.
        .fill(Signal::derive(move || {
            Paint::Solid(if hovered.get() {
                Color::from_rgb8(66, 98, 220)
            } else {
                Color::from_rgb8(96, 125, 240)
            })
        }))
        .on_click(on_click)
        .on_hover(move |value| set_hovered.set(value))
        .children(|cx| cx.text(label).build())
}

/// Side of the spinning square.
const SPINNER_SIZE: f64 = 120.0;
/// Turns per second, clockwise.
const SPINNER_SPEED: f64 = 0.25;

fn spinner(cx: &UiCx) -> NodeKey {
    let (turns, set_turns) = signal(0.0);
    on_frame(move |delta| set_turns.update(|turns| *turns += delta.as_secs_f64() * SPINNER_SPEED));

    // Its own transform is pre-painted again every frame; nothing is laid
    // out again.
    let transform = Signal::derive(move || {
        let half = SPINNER_SIZE / 2.0;
        // Move the top-left corner, anchored at the centre of the area, back
        // by half the size, then spin around the square's own centre.
        Affine::translate((-half, -half)) * Affine::rotate(turns.get() * TAU)
    });

    // The containing block of the square.
    cx.div()
        .size(size(percent(50), auto()))
        .position(Position::Relative)
        .children(|cx| {
            cx.div()
                .position(Position::Absolute)
                .inset(sides(percent(50), auto(), auto(), percent(50)))
                .size(size(px(SPINNER_SIZE), px(SPINNER_SIZE)))
                .border_radius(all(px(24)))
                .fill(Paint::Solid(Color::from_rgb8(240, 128, 96)))
                .transform(transform)
                .build()
        })
}
