use comet::prelude::*;
use comet_layout::style::geometry::AutoValue;
use comet_reactivity::cx::NodeBuild;
use comet_tree::node::NodeKey;
use peniko::Color;
use taffy::{AlignItems, FlexWrap, JustifyContent};

use super::{MUTED, panel, solid};

pub(super) fn showcase(cx: &UiCx) -> NodeKey {
    panel(cx, "01 / Flexbox", Color::WHITE, |cx| {
        [
            label(cx, "GROW / shared space in a 1 : 2 : 1 ratio"),
            flex_row(cx).children(|cx| {
                [
                    ("1×", 1.0, Color::from_rgb8(224, 233, 255)),
                    ("2× flexible", 2.0, Color::from_rgb8(198, 215, 255)),
                    ("1×", 1.0, Color::from_rgb8(224, 233, 255)),
                ]
                .map(|(caption, grow, color)| {
                    tile(cx, color)
                        .flex_basis(AutoValue::from(px(0)))
                        .flex_grow(grow)
                        .min_size(size(px(0), auto()))
                        .children(|cx| cx.text(caption).build())
                })
            }),
            label(cx, "ALIGN / different heights, one bottom edge"),
            flex_row(cx)
                .align_items(AlignItems::END)
                .justify_content(JustifyContent::SPACE_BETWEEN)
                .children(|cx| {
                    [("Small", 32), ("Medium", 44), ("Tall", 56)].map(|(caption, height)| {
                        tile(cx, Color::from_rgb8(222, 242, 233))
                            .size(size(auto(), px(height)))
                            .flex_grow(1.0)
                            .children(|cx| cx.text(caption).build())
                    })
                }),
            label(cx, "WRAP / resize the window to reflow these items"),
            flex_row(cx).flex_wrap(FlexWrap::Wrap).children(|cx| {
                [
                    "Layout",
                    "Text",
                    "Styles",
                    "Flexbox",
                    "Wrapping",
                    "Gaps",
                    "Alignment",
                    "Comet",
                ]
                .map(|caption| {
                    tile(cx, Color::from_rgb8(242, 238, 250))
                        .flex_basis(AutoValue::from(px(112)))
                        .flex_shrink(0.0)
                        .children(|cx| cx.text(caption).build())
                })
            }),
        ]
    })
}

fn flex_row(cx: &UiCx) -> NodeBuild<'_> {
    cx.div().flex().gap(size(px(8), px(8)))
}

fn label(cx: &UiCx, value: &str) -> NodeKey {
    cx.div()
        .font_size(px(11))
        .font_weight(parley::FontWeight::SEMI_BOLD)
        .text_color(solid(MUTED))
        .padding(sides(px(10), px(0), px(6), px(0)))
        .children(|cx| cx.text(value).build())
}

/// A tile, left open for more styles and its caption.
fn tile(cx: &UiCx, color: Color) -> NodeBuild<'_> {
    cx.div()
        .fill(solid(color))
        .border_radius(all(px(6)))
        .padding(axis(px(8), px(10)))
        .font_size(px(13))
        .font_weight(parley::FontWeight::SEMI_BOLD)
}
