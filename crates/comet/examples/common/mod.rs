use anyrender::Paint;
use comet_layout2::style::{
    bx::*,
    geometry::{Rect, StyleUnit},
};
use comet_renderer::style::{Fill, TextColor};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use peniko::Color;

pub fn sample_ui() -> (Ui, NodeKey) {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (TextColor(Paint::Solid(Color::BLACK)),));
    let text0 = ui.create(Node::Text, (Text("sample ".into()),));
    let text1 = ui.create(Node::Text, (Text(" text".into()),));
    let inner = ui.create(Node::Text, (Text("Inline box with emoji🎉".into()),));
    let span = ui.create(
        Node::Box,
        (
            DisplayOuter::Inline,
            Fill(Paint::Solid(Color::from_rgb8(255, 160, 160))),
            FontSize(StyleUnit::Em(1.2)),
        ),
    );
    let badge = ui.create(
        Node::Box,
        (
            DisplayOuter::Inline,
            DisplayInner::FlowRoot,
            Fill(Paint::Solid(Color::from_rgb8(255, 255, 0))),
            FontSize(StyleUnit::Px(14.0)),
        ),
    );
    let block = ui.create(
        Node::Box,
        (
            Padding(Rect::new_axis(StyleUnit::Px(16.0), StyleUnit::Px(16.0))),
            Fill(Paint::Solid(Color::from_rgb8(0, 255, 0))),
        ),
    );
    let block_text = ui.create(
        Node::Text,
        (Text("Block element occupying entire row".into()),),
    );
    let ending = ui.create(
        Node::Text,
        (
            Text("Hello comet world!".into()),
            Fill(Paint::Solid(Color::from_rgb8(200, 200, 200))),
            TextColor(Paint::Solid(Color::from_rgb8(30, 50, 160))),
        ),
    );

    ui.append(span, text0);
    ui.append(span, badge);
    ui.append(span, text1);
    ui.append(root, span);
    ui.append(badge, inner);
    ui.append(root, block);
    ui.append(block, block_text);
    ui.append(root, ending);

    (ui, root)
}
