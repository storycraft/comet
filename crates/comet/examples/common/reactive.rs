use core::f64::consts::TAU;

use comet::prelude::*;
use comet_layout::style::{parley, taffy::Position};
use comet_layout_flex::style::taffy::AlignItems;
use comet_tree::node::NodeKey;
use kurbo::Affine;
use peniko::Color;

use super::{MUTED, panel, solid};

/// Steps of the progress bar.
const STEPS: u32 = 8;
/// Side of the spinning square.
const SPINNER_SIZE: f64 = 56.0;
/// Turns per second, clockwise.
const SPINNER_SPEED: f64 = 0.5;

pub(super) fn showcase(cx: &UiCx) -> NodeKey {
    panel(
        cx,
        "08 / Reactivity",
        Color::from_rgb8(255, 246, 238),
        |cx| counter(cx).into_iter().chain(spinner(cx)),
    )
}

/// A button counting clicks. The label changes lay out the text again; the
/// bar width lays out its box again.
fn counter(cx: &UiCx) -> impl IntoView + use<> {
    let (count, set_count) = signal(0u32);

    let caption = caption(cx, "Signals / text and layout follow the count");
    let controls = cx
        .div()
        .flex()
        .align_items(AlignItems::CENTER)
        .gap(size(px(12), px(12)))
        .children(|cx| {
            [
                button(cx, Signal::derive(|| "Count up".to_owned()), move || {
                    set_count.update(|count| *count += 1)
                }),
                cx.text_with(move || format!("Clicked {} times", count.get()))
                    .font_weight(parley::FontWeight::SEMI_BOLD)
                    .build(),
            ]
        });

    // Progress through the steps, starting over once full.
    let progress = cx
        .div()
        .role(Role::ProgressIndicator)
        .size(size(auto(), px(10)))
        .margin(sides(px(10), px(0), px(0), px(0)))
        .fill(solid(Color::from_rgb8(255, 226, 204)))
        .border_radius(all(px(5)))
        .children(|cx| {
            cx.div()
                .size(Signal::derive(move || {
                    let step = count.get() % (STEPS + 1);
                    size(percent(step as f64 * 100.0 / STEPS as f64), percent(100))
                }))
                .fill(solid(Color::from_rgb8(240, 128, 96)))
                .border_radius(all(px(5)))
                .build()
        });

    [caption, controls, progress]
}

/// A square spinning while playing. Its transform is pre-painted again every
/// frame; nothing is laid out again. Frames stop while paused.
fn spinner(cx: &UiCx) -> impl IntoView + use<> {
    let (playing, set_playing) = signal(true);
    let (turns, set_turns) = signal(0.0);
    on_frame_while(playing, move |_, frame| {
        set_turns.update(|turns| *turns += frame.delta.as_secs_f64() * SPINNER_SPEED)
    });

    let caption = caption(cx, "Frames / a transform animated by a signal");
    let controls = cx
        .div()
        .flex()
        .align_items(AlignItems::CENTER)
        .gap(size(px(24), px(24)))
        .children(|cx| {
            [
                button(
                    cx,
                    Signal::derive(move || if playing.get() { "Pause" } else { "Play" }.to_owned()),
                    move || set_playing.update(|playing| *playing = !*playing),
                ),
                // The containing block of the square, which is centred in it
                // with a transform.
                cx.div()
                    .position(Position::Relative)
                    .size(size(px(SPINNER_SIZE * 1.5), px(SPINNER_SIZE * 1.5)))
                    .children(|cx| {
                        let half = SPINNER_SIZE / 2.0;
                        cx.div()
                            .position(Position::Absolute)
                            .inset(sides(percent(50), auto(), auto(), percent(50)))
                            .size(size(px(SPINNER_SIZE), px(SPINNER_SIZE)))
                            .border_radius(all(px(12)))
                            .fill(solid(Color::from_rgb8(240, 128, 96)))
                            // Move the top-left corner, at the centre, back by
                            // half the size, then spin around the square's own
                            // centre.
                            .transform(Signal::derive(move || {
                                Affine::translate((-half, -half))
                                    * Affine::rotate(turns.get() * TAU)
                            }))
                            .build()
                    }),
            ]
        });

    [caption, controls]
}

/// A button whose fill follows the hover. Fill changes only repaint.
fn button(cx: &UiCx, label: Signal<String>, on_click: impl Fn() + 'static) -> NodeKey {
    let (hovered, set_hovered) = signal(false);

    cx.div()
        .role(Role::Button)
        .padding(axis(px(8), px(16)))
        .border_radius(all(px(8)))
        .text_color(solid(Color::WHITE))
        .font_weight(parley::FontWeight::SEMI_BOLD)
        .fill(Signal::derive(move || {
            solid(if hovered.get() {
                Color::from_rgb8(66, 98, 220)
            } else {
                Color::from_rgb8(96, 125, 240)
            })
        }))
        .style((
            OnClick(Handler::new(move |_, _| on_click())),
            OnPointerEnter(Handler::new(move |_, _| set_hovered.set(true))),
            OnPointerLeave(Handler::new(move |_, _| set_hovered.set(false))),
        ))
        .children(|cx| cx.text_with(move || label.get()).build())
}

fn caption(cx: &UiCx, label: &str) -> NodeKey {
    cx.div()
        .font_size(px(12))
        .font_weight(parley::FontWeight::SEMI_BOLD)
        .text_color(solid(MUTED))
        .padding(sides(px(8), px(0), px(6), px(0)))
        .children(|cx| cx.text(label).build())
}
