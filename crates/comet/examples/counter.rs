//! A reactive counter in a window: click the button to count up. A rounded
//! square spins on the right, driven by a frame handler.

#[allow(unused)]
mod common;

use core::f64::consts::TAU;

use anyrender::Paint;
use comet::{prelude::*, winit::window::WindowAttributes};
use comet_layout::style::geometry::Point;
use comet_tree::node::NodeKey;
use kurbo::Affine;
use peniko::Color;
use taffy::{AlignItems, FlexDirection, JustifyContent};
use taffy::{Overflow, Position};

use crate::common::button;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    comet::run(WindowAttributes::default().with_title("comet counter"), app)
}

/// The counter in a scroll container filling the window, which scrolls
/// when it overflows.
fn app(cx: &UiCx) -> NodeKey {
    cx.div()
        .size(size(vw(100), vh(100)))
        .overflow(Point::new(Overflow::Scroll, Overflow::Scroll))
        .children(content)
}

fn content(cx: &UiCx) -> NodeKey {
    // A flex root shrinks to its content without a width; fill the window so
    // the halves below have a width to take.
    cx.div()
        .role(Role::Main)
        .flex()
        .size(size(vw(100), auto()))
        .min_size(size(auto(), vh(100)))
        .fill(Paint::Solid(Color::from_rgb8(239, 242, 247)))
        .text_color(Paint::Solid(Color::from_rgb8(36, 46, 63)))
        .children(|cx| [counter(cx), spinner(cx)])
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
            [
                // Text changes lay out again.
                cx.text_with(move || format!("Clicked {} times", count.get()))
                    .font_size(px(28))
                    .build(),
                button(cx, "Count up", move || {
                    set_count.update(|count| *count += 1)
                }),
            ]
        })
}

/// Side of the spinning square.
const SPINNER_SIZE: f64 = 120.0;
/// Turns per second, clockwise.
const SPINNER_SPEED: f64 = 0.25;

fn spinner(cx: &UiCx) -> NodeKey {
    let (turns, set_turns) = signal(0.0);
    on_frame(move |_, frame| {
        set_turns.update(|turns| *turns += frame.delta.as_secs_f64() * SPINNER_SPEED)
    });

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
