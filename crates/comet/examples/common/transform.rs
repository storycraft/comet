use anyrender::Paint;
use comet_layout::style::{
    bx::{DisplayInner, FontSize, FontWeight, Margin, Overflow, Padding, Size as BoxSize},
    geometry::{AutoValue, Point, Rect, Size as LayoutSize, StyleUnit},
    parley,
    taffy::Overflow as Clip,
};
use comet_layout_flex::{Flex, styles::Gap};
use comet_paint::style::{BorderRadius, Fill, TextColor, Transform, TransformOrigin};
use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};
use kurbo::Affine;
use peniko::Color;

use super::{panel, text};

pub(super) fn showcase(ui: &mut Ui, root: NodeKey) {
    let panel = panel(ui, root, "06 / Transforms", Color::from_rgb8(250, 240, 246));

    // Transforms do not move the layout: the cards keep their row slots.
    let row = ui.create(
        Node::Box,
        (
            DisplayInner::Container(Box::new(Flex)),
            Gap(LayoutSize::new(StyleUnit::Px(16.0), StyleUnit::Px(16.0))),
            Padding(Rect::new_axis(StyleUnit::Px(14.0), StyleUnit::Px(8.0))),
        ),
    );
    ui.append(panel, row);

    let rotated = card(ui, row, "rotate 8°", Color::from_rgb8(255, 214, 231));
    ui.set_props(rotated, (Transform(Affine::rotate(8f64.to_radians())),));

    // Scaled from the top left corner instead of the centre.
    let scaled = card(ui, row, "scale · top left", Color::from_rgb8(224, 233, 255));
    ui.set_props(
        scaled,
        (
            Transform(Affine::scale(0.8)),
            TransformOrigin(Point::new(StyleUnit::ZERO, StyleUnit::ZERO)),
        ),
    );

    let skewed = card(ui, row, "skew", Color::from_rgb8(222, 242, 233));
    ui.set_props(skewed, (Transform(Affine::skew(-0.3, 0.0)),));

    // Nested transforms multiply, and text colors are inherited through them.
    let outer = card(ui, panel, "rotated parent", Color::from_rgb8(236, 228, 255));
    ui.set_props(
        outer,
        (
            BoxSize(LayoutSize::new(px(260.0), AutoValue::Auto)),
            Margin(Rect::new_axis(px(18.0), px(16.0))),
            TextColor(Paint::Solid(Color::from_rgb8(96, 65, 177))),
            Transform(Affine::rotate((-4f64).to_radians())),
        ),
    );
    let inner = card(
        ui,
        outer,
        "translated child",
        Color::from_rgb8(212, 198, 250),
    );
    ui.set_props(
        inner,
        (
            BoxSize(LayoutSize::new(px(170.0), AutoValue::Auto)),
            Margin(Rect::new_invidiual(px(8.0), px(0.0), px(0.0), px(0.0))),
            Transform(Affine::translate((60.0, 0.0))),
        ),
    );

    // Overflow clips transformed contents to the stage.
    let stage = ui.create(
        Node::Box,
        (
            BoxSize(LayoutSize::new(AutoValue::Auto, px(64.0))),
            Overflow(Point::new(Clip::Hidden, Clip::Hidden)),
            Padding(Rect::new_all(StyleUnit::Px(10.0))),
            Fill(Paint::Solid(Color::WHITE)),
            BorderRadius(Rect::new_all(StyleUnit::Px(10.0))),
        ),
    );
    ui.append(panel, stage);
    let strip = card(
        ui,
        stage,
        "overflow clips this rotated strip at the stage edges",
        Color::from_rgb8(255, 222, 161),
    );
    ui.set_props(
        strip,
        (
            BoxSize(LayoutSize::new(px(640.0), AutoValue::Auto)),
            Transform(Affine::rotate((-5f64).to_radians())),
            TransformOrigin(Point::new(StyleUnit::Px(200.0), StyleUnit::Percent(0.5))),
        ),
    );
}

fn card(ui: &mut Ui, parent: NodeKey, label: &str, color: Color) -> NodeKey {
    let card = ui.create(
        Node::Box,
        (
            BoxSize(LayoutSize::new(px(130.0), AutoValue::Auto)),
            Padding(Rect::new_axis(StyleUnit::Px(10.0), StyleUnit::Px(12.0))),
            Fill(Paint::Solid(color)),
            BorderRadius(Rect::new_all(StyleUnit::Px(8.0))),
            FontSize(StyleUnit::Px(14.0)),
            FontWeight(parley::FontWeight::SEMI_BOLD),
        ),
    );
    ui.append(parent, card);
    text(ui, card, label);
    card
}

fn px(value: f64) -> AutoValue<StyleUnit> {
    AutoValue::Value(StyleUnit::Px(value))
}
