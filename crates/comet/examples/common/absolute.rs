use anyrender::Paint;
use comet_layout::style::{
    bx::{FontSize, FontWeight, Inset, Margin, MinSize, Padding, Position, Size as BoxSize},
    geometry::{AutoValue, Rect, Size as LayoutSize, StyleUnit},
    parley,
    taffy::Position as Placement,
};
use comet_paint::style::{BorderRadius, Fill, TextColor};
use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};
use peniko::Color;

use super::{panel, text};

pub(super) fn showcase(ui: &mut Ui, root: NodeKey) {
    let panel = panel(
        ui,
        root,
        "05 / Absolute positioning",
        Color::from_rgb8(236, 246, 250),
    );

    // Positioned boxes are containing blocks for their absolute descendants.
    let stage = ui.create(
        Node::Box,
        (
            Position(Placement::Relative),
            MinSize(LayoutSize::new(AutoValue::Auto, px(210.0))),
            // Room above and below the flow for the corner tags.
            Padding(Rect::new_axis(StyleUnit::Px(44.0), StyleUnit::Px(14.0))),
            Fill(Paint::Solid(Color::WHITE)),
            BorderRadius(Rect::new_all(StyleUnit::Px(10.0))),
        ),
    );
    ui.append(panel, stage);
    text(
        ui,
        stage,
        "Absolute boxes leave the flow, so this text and the cards below are laid out as if the tags were not there.",
    );

    // A relative card anchors its own absolute badge.
    let relative = card(ui, stage, "Relative card", Color::from_rgb8(224, 233, 255));
    ui.set_props(relative, (Position(Placement::Relative),));
    let counter = tag(
        ui,
        relative,
        "3",
        Color::from_rgb8(229, 72, 77),
        Rect::new_invidiual(px(-8.0), px(-8.0), AutoValue::Auto, AutoValue::Auto),
    );
    ui.set_props(
        counter,
        (
            TextColor(Paint::Solid(Color::WHITE)),
            BorderRadius(Rect::new_all(StyleUnit::Px(10.0))),
        ),
    );

    // A static card is not a containing block: its absolute child is placed
    // against the stage instead.
    let static_card = card(ui, stage, "Static card", Color::from_rgb8(255, 236, 214));
    ui.set_props(static_card, (Position(Placement::Static),));
    tag(
        ui,
        static_card,
        "declared in the static card",
        Color::from_rgb8(255, 210, 160),
        Rect::new_invidiual(AutoValue::Auto, px(12.0), px(12.0), AutoValue::Auto),
    );

    tag(
        ui,
        stage,
        "top 12 · right 12",
        Color::from_rgb8(198, 236, 220),
        Rect::new_invidiual(px(12.0), px(12.0), AutoValue::Auto, AutoValue::Auto),
    );
    tag(
        ui,
        stage,
        "bottom 12 · left 12",
        Color::from_rgb8(226, 214, 250),
        Rect::new_invidiual(AutoValue::Auto, AutoValue::Auto, px(12.0), px(12.0)),
    );
}

fn card(ui: &mut Ui, parent: NodeKey, label: &str, color: Color) -> NodeKey {
    let card = ui.create(
        Node::Box,
        (
            BoxSize(LayoutSize::new(
                AutoValue::Value(StyleUnit::Percent(0.5)),
                AutoValue::Auto,
            )),
            Padding(Rect::new_all(StyleUnit::Px(10.0))),
            Margin(Rect::new_invidiual(
                px(12.0),
                px(0.0),
                px(0.0),
                px(0.0),
            )),
            Fill(Paint::Solid(color)),
            BorderRadius(Rect::new_all(StyleUnit::Px(8.0))),
            FontWeight(parley::FontWeight::SEMI_BOLD),
        ),
    );
    ui.append(parent, card);
    text(ui, card, label);
    card
}

fn tag(
    ui: &mut Ui,
    parent: NodeKey,
    label: &str,
    color: Color,
    inset: Rect<AutoValue<StyleUnit>>,
) -> NodeKey {
    let tag = ui.create(
        Node::Box,
        (
            Position(Placement::Absolute),
            Inset(inset),
            Padding(Rect::new_axis(StyleUnit::Px(3.0), StyleUnit::Px(8.0))),
            Fill(Paint::Solid(color)),
            BorderRadius(Rect::new_all(StyleUnit::Px(6.0))),
            FontSize(StyleUnit::Px(12.0)),
            FontWeight(parley::FontWeight::SEMI_BOLD),
        ),
    );
    ui.append(parent, tag);
    text(ui, tag, label);
    tag
}

fn px(value: f64) -> AutoValue<StyleUnit> {
    AutoValue::Value(StyleUnit::Px(value))
}
