use comet::prelude::*;
use comet_geometry::AutoValue;
use comet_paint::style::UserSelect;
use comet_reactivity::cx::NodeBuild;
use comet_tree::node::NodeKey;
use peniko::Color;

use super::{MUTED, panel, solid};

pub(super) fn showcase(cx: &UiCx) -> NodeKey {
    panel(
        cx,
        "10 / Text selection",
        Color::from_rgb8(243, 248, 240),
        |cx| {
            [
                caption(cx, "Text / drag across the cards to select them all"),
                card(cx, AutoValue::Value(UserSelect::Text)).children(|cx| {
                    cx.text("Selectable text runs into the cards around it.")
                        .build()
                }),
                caption(cx, "None / pressing this card selects nothing"),
                card(cx, AutoValue::Value(UserSelect::None))
                    .color(AutoValue::Value(solid(MUTED)))
                    .children(|cx| {
                        cx.text("Labels like this one are left out of selections.")
                            .build()
                    }),
                caption(
                    cx,
                    "Contain / selections stay inside, and select it whole from outside",
                ),
                card(cx, AutoValue::Value(UserSelect::Contain)).children(|cx| {
                    [
                        cx.text("Drag out of this card: the selection stops at its ")
                            .build(),
                        cx.div()
                            .display_outer(comet_layout::style::bx::DisplayOuter::Inline)
                            .font_weight(parley::FontWeight::SEMI_BOLD)
                            .children(|cx| cx.text("edges").build()),
                        cx.text(".").build(),
                    ]
                }),
                caption(cx, "Colors / selection fill and text color per node"),
                card(cx, AutoValue::Auto)
                    .selection_fill(AutoValue::Value(solid(Color::from_rgb8(31, 111, 87))))
                    .selection_color(AutoValue::Value(solid(Color::WHITE)))
                    .children(|cx| cx.text("Selected text here is white on green.").build()),
            ]
        },
    )
}

/// A white card of text, selected as `select` says.
fn card(cx: &UiCx, select: AutoValue<UserSelect>) -> NodeBuild<'_> {
    cx.div()
        .selectable(select)
        .background(solid(Color::WHITE))
        .border_radius(all(px(8)))
        .padding(axis(px(10), px(12)))
}

fn caption(cx: &UiCx, label: &str) -> NodeKey {
    cx.div()
        .font_size(px(12))
        .font_weight(parley::FontWeight::SEMI_BOLD)
        .color(AutoValue::Value(solid(MUTED)))
        .padding(sides(px(10), px(0), px(6), px(0)))
        .children(|cx| cx.text(label).build())
}
