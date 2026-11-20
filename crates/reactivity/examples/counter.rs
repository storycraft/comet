//! A reactive counter in a window: click the button to count up. A rounded
//! square spins on the right, driven by a time signal.
//!
//! The window integration lives in `common`.

mod common;

use core::f64::consts::TAU;

use anyrender::Paint;
use comet_layout::style::{parley, taffy::Position};
use comet_layout_flex::styles::taffy::{AlignItems, FlexDirection, JustifyContent};
use comet_reactivity::prelude::*;
use comet_tree::node::NodeKey;
use kurbo::Affine;
use peniko::Color;
use reactive_graph::{
    signal::signal,
    traits::{Get, Set, Update},
    wrappers::read::Signal,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    common::run("comet-reactivity counter", app)
}

fn app(cx: &UiCx) -> NodeKey {
    // A flex root shrinks to its content without a width; fill the window so
    // the halves below have a width to take.
    cx.div()
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
            button(cx, "Count up", move || set_count.update(|count| *count += 1));
        })
}

fn button(cx: &UiCx, label: &str, on_click: impl Fn() + 'static) -> NodeKey {
    let (hovered, set_hovered) = signal(false);

    let button = cx
        .div()
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
        .children(|cx| cx.text(label).build());

    common::on_click(button, on_click);
    common::on_hover(button, move |value| set_hovered.set(value));

    button
}

/// Side of the spinning square.
const SPINNER_SIZE: f64 = 120.0;
/// Turns per second, clockwise.
const SPINNER_SPEED: f64 = 0.25;

fn spinner(cx: &UiCx) -> NodeKey {
    let (seconds, set_seconds) = signal(0.0);
    common::on_frame(move |elapsed| set_seconds.set(elapsed.as_secs_f64()));

    // Its own transform is pre-painted again every frame; nothing is laid
    // out again.
    let transform = Signal::derive(move || {
        let half = SPINNER_SIZE / 2.0;
        // Move the top-left corner, anchored at the centre of the area, back
        // by half the size, then spin around the square's own centre.
        Affine::translate((-half, -half)) * Affine::rotate(seconds.get() * SPINNER_SPEED * TAU)
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
