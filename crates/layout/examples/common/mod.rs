use comet_layout::style::bx::{DisplayInner, DisplayOuter};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

/// Shared sample, including handles used to demonstrate incremental updates.
pub struct SampleUi {
    pub ui: Ui,
    pub root: NodeKey,
    pub greeting: NodeKey,
    pub interruption: NodeKey,
}

pub fn sample_ui() -> SampleUi {
    let mut ui = Ui::new();

    // Text is always inline flow; a heading needs an explicit block element.
    let root = ui.create(Node::Box, ());
    let heading = block_text(&mut ui, "Welcome to Comet");
    let paragraph = ui.create(Node::Box, ());
    ui.append(root, heading);
    ui.append(root, paragraph);

    // The block splits this span into fragments linked by prev/next continuations.
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let greeting = inline_text(&mut ui, "Hello, 세계! ");
    let interruption = block_text(&mut ui, "A block inside an inline span.");
    let continuation = inline_text(&mut ui, "The inline span continues. ");
    ui.append(paragraph, span);
    for child in [greeting, interruption, continuation] {
        ui.append(span, child);
    }

    // This inline block owns a separate flow, without splitting the outer run.
    let badge = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let badge_text = inline_text(&mut ui, "inline-block");
    let ending = inline_text(&mut ui, " End.");
    ui.append(badge, badge_text);
    ui.append(paragraph, badge);
    ui.append(paragraph, ending);

    SampleUi {
        ui,
        root,
        greeting,
        interruption,
    }
}

fn inline_text(ui: &mut Ui, text: &str) -> NodeKey {
    ui.create(Node::Text, (Text(text.into()),))
}

fn block_text(ui: &mut Ui, text: &str) -> NodeKey {
    let block = ui.create(Node::Box, ());
    let text = inline_text(ui, text);

    ui.append(block, text);

    block
}
