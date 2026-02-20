use core::error::Error;

use anyrender::Paint;
use color::AlphaColor;
use comet::run;
use comet_div::prelude::*;
use comet_layout::style::*;
use comet_renderer::style::*;
use taffy::{LengthPercentage, Rect};

fn main() -> Result<(), Box<dyn Error>> {
    let mut ui = Ui::new();
    let root = ui.create(Node::Div, ());
    let text0 = ui.create(Node::Text("sample".to_string()), ());
    let text1 = ui.create(Node::Text("text".to_string()), ());
    let inner = ui.create(Node::Text("Inline\nbox\nwith emoji🎉".to_string()), ());
    let div = ui.create(
        Node::Div,
        (
            DisplayOuter::Inline,
            Fill(Paint::Solid(AlphaColor::from_rgb8(255, 0, 0))),
            FontSize(StyleUnit::Em(1.2)),
        ),
    );
    let div1 = ui.create(
        Node::Div,
        (
            DisplayOuter::Inline,
            DisplayInner::FlowRoot,
            Fill(Paint::Solid(AlphaColor::from_rgb8(255, 255, 0))),
            FontSize(StyleUnit::Px(14.0)),
        ),
    );

    let div2 = ui.create(
        Node::Div,
        (
            Padding(Rect {
                left: LengthPercentage::length(16.0),
                top: LengthPercentage::length(16.0),
                bottom: LengthPercentage::length(16.0),
                right: LengthPercentage::length(16.0),
            }),
            Fill(Paint::Solid(AlphaColor::from_rgb8(0, 255, 0))),
        ),
    );

    let inner2 = ui.create(Node::Text("Block element occupying entire row".to_string()), ());
    let text2 = ui.create(
        Node::Text("Hello comet world!".to_string()),
        (Fill(Paint::Solid(AlphaColor::from_rgb8(200, 200, 200))),),
    );
    ui.append(div, text0);
    ui.append(div, div1);
    ui.append(div, text1);
    ui.append(root, div);
    ui.append(div1, inner);
    ui.append(root, div2);
    ui.append(div2, inner2);
    ui.append(root, text2);

    run(ui, root)?;
    Ok(())
}
