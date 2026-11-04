use anyrender::Paint;
use comet_layout::style::{
    bx::*,
    geometry::{AutoValue, Rect, Size as LayoutSize, StyleUnit},
    parley,
};
use comet_layout_flex::{Flex, styles as flex};
use comet_renderer::style::{BorderRadius, Fill, TextColor};
use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};
use peniko::Color;

use super::{panel, text};

pub(super) fn showcase(ui: &mut Ui, root: NodeKey) {
    let panel = panel(ui, root, "01 / Flexbox", Color::WHITE);

    label(ui, panel, "GROW / shared space in a 1 : 2 : 1 ratio");

    let row = flex_row(ui, panel);

    for (caption, grow, color) in [
        ("1×", 1.0, Color::from_rgb8(224, 233, 255)),
        ("2× flexible", 2.0, Color::from_rgb8(198, 215, 255)),
        ("1×", 1.0, Color::from_rgb8(224, 233, 255)),
    ] {
        let card = tile(ui, row, caption, color);
        ui.set_props(
            card,
            (
                flex::FlexBasis(AutoValue::Value(StyleUnit::ZERO)),
                flex::FlexGrow(grow),
                MinSize(LayoutSize::new(
                    AutoValue::Value(StyleUnit::ZERO),
                    AutoValue::Auto,
                )),
            ),
        );
    }

    label(ui, panel, "ALIGN / different heights, one bottom edge");

    let row = flex_row(ui, panel);
    ui.set_props(
        row,
        (
            flex::AlignItems(taffy::AlignItems::END),
            flex::JustifyContent(taffy::JustifyContent::SPACE_BETWEEN),
        ),
    );

    for (caption, height) in [("Small", 32.0), ("Medium", 44.0), ("Tall", 56.0)] {
        let card = tile(ui, row, caption, Color::from_rgb8(222, 242, 233));
        ui.set_props(
            card,
            (
                Size(LayoutSize::new(
                    AutoValue::Auto,
                    AutoValue::Value(StyleUnit::Px(height)),
                )),
                flex::FlexGrow(1.0),
            ),
        );
    }

    label(ui, panel, "WRAP / resize the window to reflow these items");

    let row = flex_row(ui, panel);
    ui.set_props(row, (flex::FlexWrap(taffy::FlexWrap::Wrap),));

    for caption in [
        "Layout",
        "Text",
        "Styles",
        "Flexbox",
        "Wrapping",
        "Gaps",
        "Alignment",
        "Comet",
    ] {
        let chip = tile(ui, row, caption, Color::from_rgb8(242, 238, 250));
        ui.set_props(
            chip,
            (
                flex::FlexBasis(AutoValue::Value(StyleUnit::Px(112.0))),
                flex::FlexShrink(0.0),
            ),
        );
    }
}

fn flex_row(ui: &mut Ui, parent: NodeKey) -> NodeKey {
    let row = ui.create(
        Node::Box,
        (
            DisplayInner::Container(Box::new(Flex)),
            flex::Gap(LayoutSize::new(StyleUnit::Px(8.0), StyleUnit::Px(8.0))),
        ),
    );
    ui.append(parent, row);

    row
}

fn label(ui: &mut Ui, parent: NodeKey, value: &str) {
    let label = ui.create(
        Node::Box,
        (
            FontSize(StyleUnit::Px(11.0)),
            FontWeight(parley::FontWeight::SEMI_BOLD),
            TextColor(Paint::Solid(Color::from_rgb8(97, 109, 128))),
            Padding(Rect::new_invidiual(
                StyleUnit::Px(10.0),
                StyleUnit::ZERO,
                StyleUnit::Px(6.0),
                StyleUnit::ZERO,
            )),
        ),
    );
    ui.append(parent, label);
    text(ui, label, value);
}

fn tile(ui: &mut Ui, parent: NodeKey, caption: &str, color: Color) -> NodeKey {
    let tile = ui.create(
        Node::Box,
        (
            Fill(Paint::Solid(color)),
            BorderRadius(Rect::new_all(StyleUnit::Px(6.0))),
            Padding(Rect::new_axis(StyleUnit::Px(8.0), StyleUnit::Px(10.0))),
            FontSize(StyleUnit::Px(13.0)),
            FontWeight(parley::FontWeight::SEMI_BOLD),
        ),
    );
    ui.append(parent, tile);
    text(ui, tile, caption);

    tile
}
