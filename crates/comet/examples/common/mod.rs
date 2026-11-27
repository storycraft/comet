mod absolute;
mod flex;
mod layers;
mod reactive;
mod scroll;

use std::rc::Rc;

use anyrender::Paint;
use comet::{
    prelude::*,
    ui_events::keyboard::{Key, NamedKey},
};
use comet_layout::style::{
    bx::{DisplayInner, DisplayOuter},
    geometry::{AutoValue, Point},
};
use comet_shell::cursor::CursorIcon;
use comet_tree::node::NodeKey;
use parley::Alignment;
use peniko::Color;
use taffy::FlexWrap;
use taffy::Overflow;

pub const BACKGROUND: Color = Color::from_rgb8(239, 242, 247);
/// Color of captions and headings.
pub const MUTED: Color = Color::from_rgb8(97, 109, 128);

/// The sample UI in a scroll container filling the window.
pub fn app(cx: &UiCx) -> NodeKey {
    scroll_root(cx, sample_ui)
}

/// A scroll container filling the viewport, holding the node built by
/// `content`, which scrolls when it overflows the window.
pub fn scroll_root(cx: &UiCx, content: impl FnOnce(&UiCx) -> NodeKey) -> NodeKey {
    cx.div()
        .size(size(vw(100), vh(100)))
        .overflow(Point::new(Overflow::Scroll, Overflow::Scroll))
        .children(content)
}

/// A button whose fill follows the hover, with a ring while focused. Tab
/// moves the focus to it, and Enter, when pressed, or Space, when released,
/// clicks it. Fill and outline changes only repaint.
pub fn button(
    cx: &UiCx,
    label: impl Into<Signal<String>>,
    on_click: impl Fn() + 'static,
) -> NodeKey {
    let (hovered, set_hovered) = signal(false);
    let (focused, set_focused) = signal(false);
    let on_click = Rc::new(on_click);

    cx.div()
        .role(Role::Button)
        .focusable(true)
        .cursor(AutoValue::Value(Some(CursorIcon::Pointer)))
        // Pressing the label clicks, without selecting it.
        .selectable(AutoValue::Value(false))
        .outline_width(px(2))
        .outline_offset(px(2))
        .outline_fill(Signal::derive(move || {
            solid(if focused.get() {
                Color::from_rgb8(36, 46, 63)
            } else {
                Color::TRANSPARENT
            })
        }))
        .padding(axis(px(10), px(18)))
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
        .on_click({
            let on_click = on_click.clone();
            move |_, _| on_click()
        })
        .on_key_down({
            let on_click = on_click.clone();
            move |cx, event| match &event.key {
                Key::Named(NamedKey::Enter) => {
                    cx.prevent_default();
                    on_click();
                }
                // Clicked on release, like a pointer button.
                Key::Character(text) if text == " " => cx.prevent_default(),
                _ => {}
            }
        })
        .on_key_up(move |cx, event| {
            if matches!(&event.key, Key::Character(text) if text == " ") {
                cx.prevent_default();
                on_click();
            }
        })
        .on_focus(move |_, _| set_focused.set(true))
        .on_blur(move |_, _| set_focused.set(false))
        .on_hover(move |value| set_hovered.set(value))
        .children(|cx| {
            cx.text_with({
                let label = label.into();
                move || label.get()
            })
            .build()
        })
}

pub fn sample_ui(cx: &UiCx) -> NodeKey {
    cx.div()
        .role(Role::Main)
        .fill(solid(BACKGROUND))
        .text_color(solid(Color::from_rgb8(36, 46, 63)))
        .font_size(px(16))
        .padding(all(px(24)))
        .min_size(size(auto(), vh(100)))
        .children(|cx| {
            let heading = cx
                .div()
                .role(Role::Heading)
                .font_size(px(28))
                .font_weight(parley::FontWeight::BOLD)
                .children(|cx| cx.text("Comet / layout & text").build());

            // Sections wrap into as many columns as fit, each at least 420px
            // wide.
            let sections = cx
                .div()
                .flex()
                .flex_wrap(FlexWrap::Wrap)
                .gap(size(px(16), px(16)))
                .margin(sides(px(16), px(0), px(0), px(0)))
                .children(|cx| {
                    let [transforms, effects] = layers::showcase(cx);
                    [
                        flex::showcase(cx),
                        block_inline_sample(cx),
                        rich_text_sample(cx),
                        typography_sample(cx),
                        absolute::showcase(cx),
                        transforms,
                        effects,
                        reactive::showcase(cx),
                        scroll::showcase(cx),
                    ]
                });

            [heading, sections]
        })
}

fn block_inline_sample(cx: &UiCx) -> NodeKey {
    panel(cx, "02 / Block + inline", Color::WHITE, |cx| {
        [
            cx.text("Inline content starts here, ").build(),
            cx.div()
                .display_outer(DisplayOuter::Inline)
                .fill(solid(Color::from_rgb8(224, 233, 255)))
                .border_radius(all(px(6)))
                .children(|cx| {
                    [
                        cx.text("continues in a colored span ").build(),
                        badge(cx, "inline box", Color::from_rgb8(198, 215, 255)),
                        cx.text(" and wraps naturally with the available width.")
                            .build(),
                        // A block inside an inline span splits the surrounding
                        // inline flow.
                        cx.div()
                            .padding(all(px(12)))
                            .fill(solid(Color::from_rgb8(222, 242, 233)))
                            .border_radius(all(px(8)))
                            .text_align(Alignment::Center)
                            .children(|cx| {
                                [
                                    cx.text("Nested block / takes a full row, with ").build(),
                                    badge(
                                        cx,
                                        "its own inline box",
                                        Color::from_rgb8(182, 223, 207),
                                    ),
                                ]
                            }),
                        cx.text("The same inline span resumes after the block.")
                            .build(),
                    ]
                }),
            cx.text(" Then the outer text continues.").build(),
        ]
    })
}

fn rich_text_sample(cx: &UiCx) -> NodeKey {
    panel(
        cx,
        "03 / Rich text + inline block",
        Color::from_rgb8(255, 250, 240),
        |cx| {
            [
                cx.text("Regular text, ").build(),
                cx.text("bold emphasis")
                    .font_weight(parley::FontWeight::BOLD)
                    .build(),
                cx.text(", ").build(),
                cx.text("a softer italic voice")
                    .font_style(parley::FontStyle::Italic)
                    .build(),
                cx.text(", and ").build(),
                cx.text("color and scale")
                    .font_size(em(1.35))
                    .font_weight(parley::FontWeight::SEMI_BOLD)
                    .text_color(solid(Color::from_rgb8(96, 65, 177)))
                    .build(),
                cx.text(" share one paragraph. An ").build(),
                badge(cx, "inline block", Color::from_rgb8(255, 222, 161)),
                cx.text(" stays in the text flow, even as the lines wrap. ")
                    .build(),
                // Children inherit bold and color; the nested text adds
                // italics.
                cx.div()
                    .display_outer(DisplayOuter::Inline)
                    .font_weight(parley::FontWeight::BOLD)
                    .text_color(solid(Color::from_rgb8(127, 74, 32)))
                    .fill(solid(Color::from_rgb8(255, 233, 193)))
                    .children(|cx| {
                        [
                            cx.text("Nested styles ").build(),
                            cx.text("combine naturally")
                                .font_style(parley::FontStyle::Italic)
                                .build(),
                        ]
                    }),
                cx.text(". Emoji and fallback fonts work alongside them: 🎉 안녕하세요.")
                    .build(),
            ]
        },
    )
}

fn typography_sample(cx: &UiCx) -> NodeKey {
    panel(
        cx,
        "04 / Font families + spacing",
        Color::from_rgb8(242, 238, 250),
        |cx| {
            [
                cx.div()
                    .font(parley::FontFamily::from("serif"))
                    .font_size(px(22))
                    .line_height(em(1.4))
                    .children(|cx| {
                        [
                            cx.text("A serif voice, with ").build(),
                            cx.text("room to breathe.")
                                .font_style(parley::FontStyle::Italic)
                                .build(),
                        ]
                    }),
                cx.div()
                    .role(Role::Code)
                    .font(parley::FontFamily::from("monospace"))
                    .font_size(px(14))
                    .padding(axis(px(8), px(10)))
                    .fill(solid(Color::from_rgb8(227, 220, 241)))
                    .children(|cx| {
                        [
                            cx.text("let ")
                                .font_weight(parley::FontWeight::BOLD)
                                .text_color(solid(Color::from_rgb8(96, 65, 177)))
                                .build(),
                            cx.text("message = ").build(),
                            cx.text("\"Hello, Comet!\"")
                                .text_color(solid(Color::from_rgb8(31, 111, 87)))
                                .build(),
                            cx.text(";").build(),
                        ]
                    }),
                cx.div()
                    .font_size(px(12))
                    .font_weight(parley::FontWeight::SEMI_BOLD)
                    .letter_spacing(px(2))
                    .word_spacing(px(4))
                    .padding(sides(px(12), px(0), px(0), px(0)))
                    .children(|cx| cx.text("LETTER SPACING / WORD SPACING").build()),
            ]
        },
    )
}

/// A section of the sample, with a heading above the view of `body`.
fn panel<V: IntoView>(
    cx: &UiCx,
    label: &str,
    color: Color,
    body: impl FnOnce(&UiCx) -> V,
) -> NodeKey {
    cx.div()
        .role(Role::Section)
        .fill(solid(color))
        .border_radius(all(px(12)))
        .padding(all(px(16)))
        .flex_basis(AutoValue::from(px(420.0)))
        .flex_grow(1.0)
        .min_size(size(px(0), auto()))
        .children(|cx| {
            let heading = cx
                .div()
                .role(Role::Heading)
                .font_size(px(14))
                .font_weight(parley::FontWeight::SEMI_BOLD)
                .text_color(solid(MUTED))
                .padding(sides(px(0), px(0), px(12), px(0)))
                .children(|cx| cx.text(label).build());

            heading.into_iter().chain(body(cx))
        })
}

fn badge(cx: &UiCx, label: &str, color: Color) -> NodeKey {
    cx.div()
        .display_outer(DisplayOuter::Inline)
        .display_inner(DisplayInner::FlowRoot)
        .fill(solid(color))
        .border_radius(all(px(6)))
        .padding(axis(px(2), px(6)))
        .children(|cx| [cx.text("✨ ").build(), cx.text(label).build()])
}

fn solid(color: Color) -> Paint {
    Paint::Solid(color)
}
