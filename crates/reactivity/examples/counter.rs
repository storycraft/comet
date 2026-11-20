//! A reactive counter in a window: click the button to count up.
//!
//! The window integration lives in `common`.

mod common;

use anyrender::Paint;
use comet_layout::style::{
    geometry::{AutoValue, Rect, Size, StyleUnit},
    parley,
};
use comet_layout_flex::styles::taffy::{AlignItems, FlexDirection, JustifyContent};
use comet_reactivity::{FlexStyled, Styled, UiCx};
use comet_tree::node::NodeKey;
use peniko::Color;
use reactive_graph::{
    signal::signal,
    traits::{Get, Set, Update},
    wrappers::read::Signal,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    common::run("comet-reactivity counter", counter)
}

fn counter(cx: &UiCx) -> NodeKey {
    let (count, set_count) = signal(0);

    cx.div()
        .flex()
        .flex_direction(FlexDirection::Column)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .gap(Size::new(StyleUnit::Px(16.0), StyleUnit::Px(16.0)))
        .min_size(Size::new(AutoValue::Auto, AutoValue::Value(StyleUnit::Vh(1.0))))
        .fill(Paint::Solid(Color::from_rgb8(239, 242, 247)))
        .text_color(Paint::Solid(Color::from_rgb8(36, 46, 63)))
        // Text changes lay out again.
        .child(move |cx| {
            cx.text_with(move || format!("Clicked {} times", count.get()))
                .font_size(StyleUnit::Px(28.0))
                .build()
        })
        .child(move |cx| button(cx, "Count up", move || set_count.update(|count| *count += 1)))
        .build()
}

fn button(cx: &UiCx, label: &str, on_click: impl Fn() + 'static) -> NodeKey {
    let (hovered, set_hovered) = signal(false);

    let button = cx
        .div()
        .padding(Rect::new_axis(StyleUnit::Px(10.0), StyleUnit::Px(18.0)))
        .border_radius(Rect::new_all(StyleUnit::Px(8.0)))
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
        .child(|cx| cx.text(label).build())
        .build();

    common::on_click(button, on_click);
    common::on_hover(button, move |value| set_hovered.set(value));

    button
}
