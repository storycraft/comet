mod absolute;
mod flex;

use anyrender::Paint;
use comet_layout::style::{
    bx::*,
    geometry::{AutoValue, Rect, Size as LayoutSize, StyleUnit},
    parley::{self, Alignment},
};
use comet_layout_flex::{Flex, styles as flex_style};
use comet_paint::style::{BorderRadius, Fill, TextColor};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use peniko::Color;

pub const BACKGROUND: Color = Color::from_rgb8(239, 242, 247);

pub fn sample_ui() -> (Ui, NodeKey) {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            Fill(Paint::Solid(BACKGROUND)),
            TextColor(Paint::Solid(Color::from_rgb8(36, 46, 63))),
            FontSize(StyleUnit::Px(16.0)),
            Padding(Rect::new_all(StyleUnit::Px(24.0))),
            MinSize(LayoutSize::new(
                AutoValue::Auto,
                AutoValue::Value(StyleUnit::Vh(1.0)),
            )),
        ),
    );
    let title = ui.create(
        Node::Box,
        (
            FontSize(StyleUnit::Px(28.0)),
            FontWeight(parley::FontWeight::BOLD),
        ),
    );
    ui.append(root, title);
    text(&mut ui, title, "Comet / layout & text");

    // Sections wrap into as many columns as fit, each at least 420px wide.
    let sections = ui.create(
        Node::Box,
        (
            DisplayInner::Container(Box::new(Flex)),
            flex_style::FlexWrap(flex_style::taffy::FlexWrap::Wrap),
            flex_style::Gap(LayoutSize::new(StyleUnit::Px(16.0), StyleUnit::Px(16.0))),
            Margin(Rect::new_invidiual(
                AutoValue::Value(StyleUnit::Px(16.0)),
                AutoValue::Value(StyleUnit::ZERO),
                AutoValue::Value(StyleUnit::ZERO),
                AutoValue::Value(StyleUnit::ZERO),
            )),
        ),
    );
    ui.append(root, sections);

    flex::showcase(&mut ui, sections);
    block_inline_sample(&mut ui, sections);
    rich_text_sample(&mut ui, sections);
    typography_sample(&mut ui, sections);
    absolute::showcase(&mut ui, sections);

    (ui, root)
}

fn block_inline_sample(ui: &mut Ui, root: NodeKey) {
    let panel = panel(ui, root, "02 / Block + inline", Color::WHITE);
    text(ui, panel, "Inline content starts here, ");
    let span = ui.create(
        Node::Box,
        (
            DisplayOuter::Inline,
            Fill(Paint::Solid(Color::from_rgb8(224, 233, 255))),
            BorderRadius(Rect::new_all(StyleUnit::Px(6.0))),
        ),
    );
    ui.append(panel, span);
    text(ui, span, "continues in a colored span ");
    badge(ui, span, "inline box", Color::from_rgb8(198, 215, 255));
    text(ui, span, " and wraps naturally with the available width.");

    // A block inside an inline span splits the surrounding inline flow.
    let block = ui.create(
        Node::Box,
        (
            Padding(Rect::new_all(StyleUnit::Px(12.0))),
            Fill(Paint::Solid(Color::from_rgb8(222, 242, 233))),
            BorderRadius(Rect::new_all(StyleUnit::Px(8.0))),
            TextAlign(Alignment::Center),
        ),
    );
    ui.append(span, block);
    text(ui, block, "Nested block / takes a full row, with ");
    badge(
        ui,
        block,
        "its own inline box",
        Color::from_rgb8(182, 223, 207),
    );
    text(ui, span, "The same inline span resumes after the block.");
    text(ui, panel, " Then the outer text continues.");
}

fn rich_text_sample(ui: &mut Ui, root: NodeKey) {
    let panel = panel(
        ui,
        root,
        "03 / Rich text + inline block",
        Color::from_rgb8(255, 250, 240),
    );
    text(ui, panel, "Regular text, ");
    let bold = text(ui, panel, "bold emphasis");
    ui.set_props(bold, (FontWeight(parley::FontWeight::BOLD),));
    text(ui, panel, ", ");
    let italic = text(ui, panel, "a softer italic voice");
    ui.set_props(italic, (FontStyle(parley::FontStyle::Italic),));
    text(ui, panel, ", and ");
    let accent = ui.create(
        Node::Text,
        (
            Text("color and scale".into()),
            FontSize(StyleUnit::Em(1.35)),
            FontWeight(parley::FontWeight::SEMI_BOLD),
            TextColor(Paint::Solid(Color::from_rgb8(96, 65, 177))),
        ),
    );
    ui.append(panel, accent);
    text(ui, panel, " share one paragraph. An ");
    badge(ui, panel, "inline block", Color::from_rgb8(255, 222, 161));
    text(
        ui,
        panel,
        " stays in the text flow, even as the lines wrap. ",
    );

    // Children inherit bold and color; the nested text adds italics.
    let emphasis = ui.create(
        Node::Box,
        (
            DisplayOuter::Inline,
            FontWeight(parley::FontWeight::BOLD),
            TextColor(Paint::Solid(Color::from_rgb8(127, 74, 32))),
            Fill(Paint::Solid(Color::from_rgb8(255, 233, 193))),
        ),
    );
    ui.append(panel, emphasis);
    text(ui, emphasis, "Nested styles ");
    let nested = text(ui, emphasis, "combine naturally");
    ui.set_props(nested, (FontStyle(parley::FontStyle::Italic),));
    text(
        ui,
        panel,
        ". Emoji and fallback fonts work alongside them: 🎉 안녕하세요.",
    );
}

fn typography_sample(ui: &mut Ui, root: NodeKey) {
    let panel = panel(
        ui,
        root,
        "04 / Font families + spacing",
        Color::from_rgb8(242, 238, 250),
    );
    let serif = ui.create(
        Node::Box,
        (
            Font(parley::FontFamily::from("serif")),
            FontSize(StyleUnit::Px(22.0)),
            LineHeight(StyleUnit::Em(1.4)),
        ),
    );
    ui.append(panel, serif);
    text(ui, serif, "A serif voice, with ");
    let italic = text(ui, serif, "room to breathe.");
    ui.set_props(italic, (FontStyle(parley::FontStyle::Italic),));

    let code = ui.create(
        Node::Box,
        (
            Font(parley::FontFamily::from("monospace")),
            FontSize(StyleUnit::Px(14.0)),
            Padding(Rect::new_axis(StyleUnit::Px(8.0), StyleUnit::Px(10.0))),
            Fill(Paint::Solid(Color::from_rgb8(227, 220, 241))),
        ),
    );
    ui.append(panel, code);
    let keyword = text(ui, code, "let ");
    ui.set_props(
        keyword,
        (
            FontWeight(parley::FontWeight::BOLD),
            TextColor(Paint::Solid(Color::from_rgb8(96, 65, 177))),
        ),
    );
    text(ui, code, "message = ");
    let string = text(ui, code, "\"Hello, Comet!\"");
    ui.set_props(
        string,
        (TextColor(Paint::Solid(Color::from_rgb8(31, 111, 87))),),
    );
    text(ui, code, ";");

    let spaced = ui.create(
        Node::Box,
        (
            FontSize(StyleUnit::Px(12.0)),
            FontWeight(parley::FontWeight::SEMI_BOLD),
            LetterSpacing(StyleUnit::Px(2.0)),
            WordSpacing(StyleUnit::Px(4.0)),
            Padding(Rect::new_invidiual(
                StyleUnit::Px(12.0),
                StyleUnit::ZERO,
                StyleUnit::ZERO,
                StyleUnit::ZERO,
            )),
        ),
    );
    ui.append(panel, spaced);
    text(ui, spaced, "LETTER SPACING / WORD SPACING");
}

fn panel(ui: &mut Ui, root: NodeKey, label: &str, color: Color) -> NodeKey {
    let panel = ui.create(
        Node::Box,
        (
            Fill(Paint::Solid(color)),
            BorderRadius(Rect::new_all(StyleUnit::Px(12.0))),
            Padding(Rect::new_all(StyleUnit::Px(16.0))),
            flex_style::FlexBasis(AutoValue::Value(StyleUnit::Px(420.0))),
            flex_style::FlexGrow(1.0),
            MinSize(LayoutSize::new(
                AutoValue::Value(StyleUnit::ZERO),
                AutoValue::Auto,
            )),
        ),
    );
    ui.append(root, panel);
    let heading = ui.create(
        Node::Box,
        (
            FontSize(StyleUnit::Px(14.0)),
            FontWeight(parley::FontWeight::SEMI_BOLD),
            TextColor(Paint::Solid(Color::from_rgb8(97, 109, 128))),
            Padding(Rect::new_invidiual(
                StyleUnit::ZERO,
                StyleUnit::ZERO,
                StyleUnit::Px(12.0),
                StyleUnit::ZERO,
            )),
        ),
    );
    ui.append(panel, heading);
    text(ui, heading, label);
    panel
}

fn badge(ui: &mut Ui, parent: NodeKey, label: &str, color: Color) {
    let badge = ui.create(
        Node::Box,
        (
            DisplayOuter::Inline,
            DisplayInner::FlowRoot,
            Fill(Paint::Solid(color)),
            BorderRadius(Rect::new_all(StyleUnit::Px(6.0))),
            Padding(Rect::new_axis(StyleUnit::Px(2.0), StyleUnit::Px(6.0))),
        ),
    );

    ui.append(parent, badge);
    text(ui, badge, "✨ ");
    text(ui, badge, label);
}

fn text(ui: &mut Ui, parent: NodeKey, value: &str) -> NodeKey {
    let node = ui.create(Node::Text, (Text(value.into()),));
    ui.append(parent, node);
    node
}
