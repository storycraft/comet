use comet::prelude::*;
use comet_geometry::{AutoValue, Point, StyleUnit};
use comet_reactivity::cx::NodeBuild;
use comet_tree::node::NodeKey;
use peniko::Color;
use taffy::FlexWrap;
use taffy::Overflow;

use super::{MUTED, panel, solid};

pub(super) fn showcase(cx: &UiCx) -> NodeKey {
    panel(
        cx,
        "09 / Scroll containers",
        Color::from_rgb8(240, 244, 255),
        |cx| {
            [
                caption(cx, "Vertical / the wheel scrolls the list"),
                vertical(cx),
                caption(cx, "Horizontal / shift or a touchpad scrolls sideways"),
                horizontal(cx),
                caption(
                    cx,
                    "Nested / the inner list scrolls first, then the outer one",
                ),
                nested(cx),
                caption(
                    cx,
                    "Hidden / clipped, not scrolled: the wheel goes to the page",
                ),
                hidden(cx),
            ]
        },
    )
}

/// A list taller than its box, scrolling vertically.
fn vertical(cx: &UiCx) -> NodeKey {
    frame(cx, px(140))
        .overflow(Point::new(Overflow::Visible, Overflow::Scroll))
        .children(|cx| rows(cx, "Row", 12, Color::from_rgb8(224, 233, 255)))
}

/// Tiles in one row wider than their box, scrolling horizontally.
fn horizontal(cx: &UiCx) -> NodeKey {
    frame(cx, auto())
        .overflow(Point::new(Overflow::Scroll, Overflow::Visible))
        .children(|cx| {
            cx.div()
                .flex()
                .flex_wrap(FlexWrap::NoWrap)
                .gap(size(px(8), px(8)))
                .padding(all(px(8)))
                .children(|cx| {
                    (1..=12)
                        .map(|index| {
                            cx.div()
                                .size(size(px(88), px(56)))
                                .flex_shrink(0.0)
                                .padding(all(px(8)))
                                .background(solid(Color::from_rgb8(255, 226, 199)))
                                .border_radius(all(px(8)))
                                .font_weight(parley::FontWeight::SEMI_BOLD)
                                .children(|cx| cx.text(format!("Tile {index}")).build())
                        })
                        .collect::<Vec<_>>()
                })
        })
}

/// A scrolling box holding another one. The inner list takes the wheel
/// until it reaches an end, then the outer box scrolls.
fn nested(cx: &UiCx) -> NodeKey {
    frame(cx, px(180))
        .overflow(Point::new(Overflow::Visible, Overflow::Scroll))
        .children(|cx| {
            let before = rows(cx, "Outer", 3, Color::from_rgb8(222, 242, 233));
            let inner = cx
                .div()
                .size(size(auto(), px(96)))
                .margin(axis(px(6), px(8)))
                .overflow(Point::new(Overflow::Visible, Overflow::Scroll))
                .background(solid(Color::WHITE))
                .border(all(px(2)))
                .border_fill(solid(Color::from_rgb8(120, 190, 160)))
                .border_radius(all(px(8)))
                .children(|cx| rows(cx, "Inner", 8, Color::from_rgb8(198, 236, 220)))
                .into_iter();
            let after = rows(cx, "Outer", 3, Color::from_rgb8(222, 242, 233));

            before
                .into_iter()
                .chain(inner)
                .chain(after)
                .collect::<Vec<_>>()
        })
}

/// A list clipped by its box without scrolling.
fn hidden(cx: &UiCx) -> NodeKey {
    frame(cx, px(72))
        .overflow(Point::new(Overflow::Hidden, Overflow::Hidden))
        .children(|cx| rows(cx, "Clipped", 6, Color::from_rgb8(236, 228, 252)))
}

/// A bordered white box of `height`.
fn frame(cx: &UiCx, height: impl Into<AutoValue<StyleUnit>>) -> NodeBuild<'_> {
    cx.div()
        .size(size(auto(), height.into()))
        .background(solid(Color::WHITE))
        .border(all(px(1)))
        .border_fill(solid(Color::from_rgb8(206, 214, 230)))
        .border_radius(all(px(10)))
}

/// `count` numbered rows, alternating `color` with white.
fn rows(cx: &UiCx, label: &str, count: u32, color: Color) -> Vec<NodeKey> {
    (1..=count)
        .map(|index| {
            let fill = if index % 2 == 1 { color } else { Color::WHITE };

            cx.div()
                .padding(axis(px(6), px(12)))
                .background(solid(fill))
                .children(|cx| cx.text(format!("{label} {index}")).build())
        })
        .collect()
}

fn caption(cx: &UiCx, label: &str) -> NodeKey {
    cx.div()
        .font_size(px(12))
        .font_weight(parley::FontWeight::SEMI_BOLD)
        .color(AutoValue::Value(solid(MUTED)))
        .padding(sides(px(10), px(0), px(6), px(0)))
        .children(|cx| cx.text(label).build())
}
