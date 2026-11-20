use comet::prelude::*;
use comet_layout::style::{
    geometry::{AutoValue, Rect, StyleUnit},
    parley,
    taffy::Position,
};
use comet_reactivity::NodeBuild;
use comet_tree::node::NodeKey;
use peniko::Color;

use super::{panel, solid};

pub(super) fn showcase(cx: &UiCx) -> NodeKey {
    panel(
        cx,
        "05 / Absolute positioning",
        Color::from_rgb8(236, 246, 250),
        |cx| {
            // Positioned boxes are containing blocks for their absolute
            // descendants.
            cx.div()
                .position(Position::Relative)
                .min_size(size(auto(), px(210)))
                // Room above and below the flow for the corner tags.
                .padding(axis(px(44), px(14)))
                .fill(solid(Color::WHITE))
                .border_radius(all(px(10)))
                .children(stage);
        },
    )
}

fn stage(cx: &UiCx) {
    cx.text(
        "Absolute boxes leave the flow, so this text and the cards below are laid out as if the tags were not there.",
    )
    .build();

    // A relative card anchors its own absolute badge.
    card(cx, Color::from_rgb8(224, 233, 255))
        .position(Position::Relative)
        .children(|cx| {
            cx.text("Relative card").build();
            tag(
                cx,
                Color::from_rgb8(229, 72, 77),
                sides(px(-8), px(-8), auto(), auto()),
            )
            .text_color(solid(Color::WHITE))
            .border_radius(all(px(10)))
            .children(|cx| cx.text("3").build());
        });

    // A static card is not a containing block: its absolute child is placed
    // against the stage instead.
    card(cx, Color::from_rgb8(255, 236, 214))
        .position(Position::Static)
        .children(|cx| {
            cx.text("Static card").build();
            tag(
                cx,
                Color::from_rgb8(255, 210, 160),
                sides(auto(), px(12), px(12), auto()),
            )
            .children(|cx| cx.text("declared in the static card").build());
        });

    tag(
        cx,
        Color::from_rgb8(198, 236, 220),
        sides(px(12), px(12), auto(), auto()),
    )
    .children(|cx| cx.text("top 12 · right 12").build());
    tag(
        cx,
        Color::from_rgb8(226, 214, 250),
        sides(auto(), auto(), px(12), px(12)),
    )
    .children(|cx| cx.text("bottom 12 · left 12").build());
}

fn card(cx: &UiCx, color: Color) -> NodeBuild<'_> {
    cx.div()
        .size(size(percent(50), auto()))
        .padding(all(px(10)))
        .margin(sides(px(12), px(0), px(0), px(0)))
        .fill(solid(color))
        .border_radius(all(px(8)))
        .font_weight(parley::FontWeight::SEMI_BOLD)
}

fn tag(cx: &UiCx, color: Color, inset: Rect<AutoValue<StyleUnit>>) -> NodeBuild<'_> {
    cx.div()
        .position(Position::Absolute)
        .inset(inset)
        .padding(axis(px(3), px(8)))
        .fill(solid(color))
        .border_radius(all(px(6)))
        .font_size(px(12))
        .font_weight(parley::FontWeight::SEMI_BOLD)
}
