use comet_layout2::{
    style::{DisplayInner, DisplayOuter},
    tree::{InlineIns, LayoutItemKey, LayoutItemTy, LayoutTree, builder::LayoutTreeBuilder},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ui = Ui::new();

    // A text element is block-level unless explicitly made inline.
    let root = ui.create(Node::Box, ());
    let heading = ui.create(Node::Text, (Text("Welcome to Comet".into()),));
    let paragraph = ui.create(Node::Box, ());
    ui.append(root, heading);
    ui.append(root, paragraph);

    // The block inside this inline span splits the paragraph's inline run.
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let greeting = inline_text(&mut ui, "Hello, 세계! ");
    let interruption = ui.create(Node::Text, (Text("A block inside an inline span.".into()),));
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

    println!("Initial layout tree (indentation shows ownership; text ranges are UTF-8 bytes):");
    print_tree(&tree, layout_root);

    // Update at the existing root. Compatible layout keys and buffers are retained.
    ui.set_props(greeting, (Text("Updated greeting! ".into()),));
    let layout_root = builder
        .build(&ui, root, &mut tree, layout_root)
        .ok_or("could not update sample UI")?;

    println!("\nAfter changing the greeting:");
    print_tree(&tree, layout_root);

    Ok(())
}

fn inline_text(ui: &mut Ui, text: &str) -> NodeKey {
    ui.create(Node::Text, (Text(text.into()), DisplayOuter::Inline))
}

fn print_tree(tree: &LayoutTree, root: LayoutItemKey) {
    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let Some(item) = tree.items.get(key) else {
            continue;
        };
        let indent = "  ".repeat(depth);
        let source = item
            .node
            .map_or_else(|| "anonymous".into(), |node| format!("source={node:?}"));

        match &item.ty {
            LayoutItemTy::Block => println!("{indent}{key:?} Block ({source})"),
            LayoutItemTy::Inline(inline) => {
                println!("{indent}{key:?} Inline ({source}) text={:?}", inline.text);

                for instruction in &inline.instructions {
                    match instruction {
                        InlineIns::Text { node, range } => {
                            println!(
                                "{indent}  | Text {node:?} {range:?} {:?}",
                                inline.text.get(range.clone())
                            );
                        }
                        InlineIns::PushInline(node) => println!("{indent}  | PushInline {node:?}"),
                        InlineIns::PopInline => println!("{indent}  | PopInline"),
                        InlineIns::InlineBlock(child) => {
                            println!("{indent}  | InlineBlock -> {child:?}")
                        }
                    }
                }
            }
        }

        pending.extend(
            tree.items
                .children(key)
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }
}
