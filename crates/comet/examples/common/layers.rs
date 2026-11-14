use std::sync::Arc;

use anyrender::{Filter as FilterGraph, Paint, filters::FilterEffect};
use comet_layout::style::{
    bx::{DisplayInner, FontSize, FontWeight, Margin, Overflow, Padding, Size as BoxSize},
    geometry::{AutoValue, Point, Rect, Size as LayoutSize, StyleUnit},
    parley,
    taffy::Overflow as Clip,
};
use comet_layout_flex::{Flex, styles::Gap};
use comet_paint::style::{
    BorderRadius, Fill, Filter, Opacity, TextColor, Transform, TransformOrigin, ZIndex,
};
use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};
use kurbo::{Affine, Vec2};
use peniko::Color;

use super::{panel, text};

pub(super) fn showcase(ui: &mut Ui, root: NodeKey) {
    let transforms = panel(ui, root, "06 / Transforms", Color::from_rgb8(250, 240, 246));
    transform_origins(ui, transforms);
    nested_transforms(ui, transforms);

    let compositing = panel(
        ui,
        root,
        "07 / Opacity, z-index + filters",
        Color::from_rgb8(240, 244, 250),
    );
    opacity(ui, compositing);
    z_index(ui, compositing);
    filters(ui, compositing);
}

/// The same rotation around different transform origins. Transforms do not
/// move the layout: the cards keep their row slots.
fn transform_origins(ui: &mut Ui, panel: NodeKey) {
    caption(ui, panel, "Transform origin");
    let row = row(ui, panel);

    for (label, x, y, color) in [
        ("top left", 0.0, 0.0, Color::from_rgb8(255, 214, 231)),
        ("centre", 0.5, 0.5, Color::from_rgb8(224, 233, 255)),
        ("bottom right", 1.0, 1.0, Color::from_rgb8(222, 242, 233)),
    ] {
        let card = card(ui, row, label, color);
        ui.set_props(
            card,
            (
                Transform(Affine::rotate(10f64.to_radians())),
                TransformOrigin(Point::new(StyleUnit::Percent(x), StyleUnit::Percent(y))),
            ),
        );
    }

    // Origins also take lengths, and combine with any transform.
    let scaled = card(ui, row, "scale · 12px", Color::from_rgb8(255, 236, 200));
    ui.set_props(
        scaled,
        (
            Transform(Affine::scale(0.8)),
            TransformOrigin(Point::new(StyleUnit::Px(12.0), StyleUnit::Px(12.0))),
        ),
    );
}

/// Opacity fades a box together with its contents, as one group.
fn opacity(ui: &mut Ui, panel: NodeKey) {
    caption(ui, panel, "Opacity");
    let row = row(ui, panel);

    for opacity in [1.0, 0.7, 0.4] {
        let card = card(
            ui,
            row,
            &format!("opacity {opacity}"),
            Color::from_rgb8(212, 198, 250),
        );
        ui.set_props(card, (Opacity(opacity),));
    }

    // The overlapping children fade as one: they do not show through each
    // other.
    let group = card(ui, row, "group 0.5", Color::from_rgb8(255, 222, 161));
    ui.set_props(group, (Opacity(0.5),));
    let first = chip(ui, group, Color::from_rgb8(229, 72, 77));
    ui.set_props(first, (Margin(Rect::new_axis(px(6.0), px(0.0))),));
    let second = chip(ui, group, Color::from_rgb8(31, 111, 87));
    ui.set_props(
        second,
        (Margin(Rect::new_invidiual(
            px(-14.0),
            px(0.0),
            px(0.0),
            px(16.0),
        )),),
    );
}

/// Overlapping cards paint by z-index. Auto z-index cards paint in tree
/// order, after negative layers and before positive ones.
fn z_index(ui: &mut Ui, panel: NodeKey) {
    caption(ui, panel, "Z-index");
    // The row starts its own layer, so negative layers paint above its
    // background instead of below the panel's.
    let row = row(ui, panel);
    ui.set_props(
        row,
        (
            Gap(LayoutSize::new(StyleUnit::ZERO, StyleUnit::ZERO)),
            ZIndex(AutoValue::Value(0)),
            Fill(Paint::Solid(Color::WHITE)),
            BorderRadius(Rect::new_all(StyleUnit::Px(10.0))),
        ),
    );

    for (index, (label, z_index, color)) in [
        ("z 2", AutoValue::Value(2), Color::from_rgb8(255, 214, 231)),
        (
            "z -1",
            AutoValue::Value(-1),
            Color::from_rgb8(224, 233, 255),
        ),
        ("auto", AutoValue::Auto, Color::from_rgb8(222, 242, 233)),
        ("z 1", AutoValue::Value(1), Color::from_rgb8(255, 236, 200)),
    ]
    .into_iter()
    .enumerate()
    {
        let card = card(ui, row, label, color);
        // Each card overlaps the previous one and sits a little lower.
        let left = if index == 0 { 0.0 } else { -40.0 };
        ui.set_props(
            card,
            (
                ZIndex(z_index),
                Margin(Rect::new_invidiual(
                    px(index as f64 * 8.0),
                    px(0.0),
                    px(0.0),
                    px(left),
                )),
            ),
        );
    }
}

/// Filters apply to a box and its contents as one group, in its local
/// coordinates.
fn filters(ui: &mut Ui, panel: NodeKey) {
    caption(ui, panel, "Filters");
    let row = row(ui, panel);

    let blurred = card(ui, row, "blur 1.5", Color::from_rgb8(212, 198, 250));
    ui.set_props(blurred, (filter(FilterEffect::blur(1.5)),));

    let shadowed = card(ui, row, "drop shadow", Color::WHITE);
    ui.set_props(
        shadowed,
        (filter(FilterEffect::drop_shadow(
            3.0,
            4.0,
            3.0,
            Color::from_rgba8(36, 46, 63, 90),
        )),),
    );

    // The filter sees the box and its children as one image: the shadow
    // follows the chip poking out of the card too.
    let group = card(ui, row, "shadow · group", Color::from_rgb8(255, 214, 231));
    ui.set_props(
        group,
        (filter(FilterEffect::drop_shadow(
            0.0,
            6.0,
            2.0,
            Color::from_rgba8(229, 72, 77, 120),
        )),),
    );
    let poking = chip(ui, group, Color::from_rgb8(229, 72, 77));
    ui.set_props(
        poking,
        (Margin(Rect::new_invidiual(
            px(6.0),
            px(0.0),
            px(0.0),
            px(96.0),
        )),),
    );

    // Offsets are in the box's local coordinates, so they turn with it.
    let offset = card(ui, row, "offset · rotated", Color::from_rgb8(222, 242, 233));
    ui.set_props(
        offset,
        (
            filter(FilterEffect::Offset(Vec2::new(12.0, 0.0))),
            Transform(Affine::rotate(20f64.to_radians())),
        ),
    );
}

fn filter(effect: FilterEffect) -> Filter {
    Filter(Arc::new(FilterGraph::single(effect)))
}

/// Nested transforms multiply, and overflow clips transformed contents.
fn nested_transforms(ui: &mut Ui, panel: NodeKey) {
    caption(ui, panel, "Nesting + clipping");

    // Text colors are inherited through transforms.
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

    // The clip also applies to the translucent strip, painting in its own
    // layer.
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
        "overflow clips this rotated, translucent strip at the stage edges",
        Color::from_rgb8(255, 222, 161),
    );
    ui.set_props(
        strip,
        (
            BoxSize(LayoutSize::new(px(640.0), AutoValue::Auto)),
            Opacity(0.8),
            Transform(Affine::rotate((-5f64).to_radians())),
            TransformOrigin(Point::new(StyleUnit::Px(200.0), StyleUnit::Percent(0.5))),
        ),
    );
}

fn caption(ui: &mut Ui, parent: NodeKey, label: &str) {
    let caption = ui.create(
        Node::Box,
        (
            FontSize(StyleUnit::Px(12.0)),
            FontWeight(parley::FontWeight::SEMI_BOLD),
            TextColor(Paint::Solid(Color::from_rgb8(97, 109, 128))),
            Padding(Rect::new_invidiual(
                StyleUnit::Px(8.0),
                StyleUnit::ZERO,
                StyleUnit::Px(4.0),
                StyleUnit::ZERO,
            )),
        ),
    );
    ui.append(parent, caption);
    text(ui, caption, label);
}

fn row(ui: &mut Ui, parent: NodeKey) -> NodeKey {
    let row = ui.create(
        Node::Box,
        (
            DisplayInner::Container(Box::new(Flex)),
            Gap(LayoutSize::new(StyleUnit::Px(16.0), StyleUnit::Px(16.0))),
            Padding(Rect::new_axis(StyleUnit::Px(10.0), StyleUnit::Px(8.0))),
        ),
    );
    ui.append(parent, row);
    row
}

fn card(ui: &mut Ui, parent: NodeKey, label: &str, color: Color) -> NodeKey {
    let card = ui.create(
        Node::Box,
        (
            BoxSize(LayoutSize::new(px(110.0), AutoValue::Auto)),
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

/// A small solid square.
fn chip(ui: &mut Ui, parent: NodeKey, color: Color) -> NodeKey {
    let chip = ui.create(
        Node::Box,
        (
            BoxSize(LayoutSize::new(px(28.0), px(20.0))),
            Fill(Paint::Solid(color)),
            BorderRadius(Rect::new_all(StyleUnit::Px(4.0))),
        ),
    );
    ui.append(parent, chip);
    chip
}

fn px(value: f64) -> AutoValue<StyleUnit> {
    AutoValue::Value(StyleUnit::Px(value))
}
