use comet_layout2::{
    style::{DisplayInner, DisplayOuter},
    tree::{LayoutItemKey, LayoutItemTy, LayoutTree, builder::LayoutTreeBuilder},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ui = Ui::new();

    // Text is always inline flow; a heading needs an explicit block element.
    let root = ui.create(Node::Box, ());
    let heading = block_text(&mut ui, "Welcome to Comet");
    let paragraph = ui.create(Node::Box, ());
    ui.append(root, heading);
    ui.append(root, paragraph);

    // The block inside this inline span splits the paragraph's inline run.
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

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let layout_root = builder
        .build_new(&ui, root, &mut tree)
        .ok_or("could not build sample UI")?;

    println!("Initial layout object tree:");
    print_tree(&ui, &tree, layout_root);

    // Text stays in the UI; updating preserves compatible layout object keys.
    ui.set_props(greeting, (Text("Updated greeting! ".into()),));
    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let layout_root = builder
        .build(&ui, root, &mut tree, layout_root)
        .ok_or("could not update sample UI")?;

    println!("\nAfter changing the greeting:");
    print_tree(&ui, &tree, layout_root);

    Ok(())
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

fn print_tree(ui: &Ui, tree: &LayoutTree, root: LayoutItemKey) {
    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let Some(item) = tree.items.get(key) else {
            continue;
        };
        let indent = "  ".repeat(depth);
        println!(
            "{indent}{key:?} {:?} children={:?}",
            item.ty, item.child_layout
        );

        if let LayoutItemTy::Text(node) = item.ty {
            let text = ui.prop::<Text>(node);

            println!(
                "{indent}  {:?}",
                text.as_deref()
                    .map(|text| text.0.as_str())
                    .unwrap_or_default()
            );
        }

        pending.extend(
            tree.items
                .children(key)
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }
}
