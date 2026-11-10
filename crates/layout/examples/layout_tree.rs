mod common;

use comet_layout::{
    input::{
        LayoutInputKey, LayoutInputKind, builder::LayoutInputTreeBuilder, tree::LayoutInputTree,
    },
    style::{
        bx::{DisplayInner, DisplayOuter, FontSize},
        geometry::{Size, StyleUnit},
    },
};
use comet_tree::{node::Text, ui::Ui};

const ROOT_FONT_SIZE: f64 = 16.0;
const VIEWPORT: Size<f64> = Size::new(800.0, 600.0);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let common::SampleUi {
        mut ui,
        root,
        greeting,
        interruption,
    } = common::sample_ui();

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let layout_root = builder
        .build(&ui, root, ROOT_FONT_SIZE, VIEWPORT, &mut tree, None)
        .ok_or("could not build sample UI")?;

    println!("Initial layout input tree (every item is flagged):");
    print_tree(&ui, &tree, layout_root);

    // Partial rebuilds only re-resolve the target item's subtree and keep the
    // returned key in place. Targets are rebuilt while they are flagged.

    // Text stays in the UI; rebuilding the text item keeps its key.
    ui.set_props(greeting, (Text("Updated greeting! ".into()),));

    let greeting_item = tree.item_for_node(greeting);
    let greeting_item = builder
        .build(
            &ui,
            greeting,
            ROOT_FONT_SIZE,
            VIEWPORT,
            &mut tree,
            greeting_item,
        )
        .ok_or("could not rebuild the greeting")?;

    println!("\nAfter rebuilding the changed greeting {greeting_item:?}:");
    print_tree(&ui, &tree, layout_root);

    // An inline-block becomes an atomic inline item with its own flow. Items only
    // describe their own node, so rebuilding the changed node is enough.
    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));

    let interruption_item = tree.item_for_node(interruption);
    let interruption_item = builder
        .build(
            &ui,
            interruption,
            ROOT_FONT_SIZE,
            VIEWPORT,
            &mut tree,
            interruption_item,
        )
        .ok_or("could not rebuild the interruption")?;

    println!("\nAfter changing the interruption {interruption_item:?} to inline-block:");
    print_tree(&ui, &tree, layout_root);

    // Text styles are resolved during building and inherited from the parent item.
    ui.set_props(interruption, (DisplayOuter::Block, DisplayInner::Flow));
    ui.set_props(interruption, (FontSize(StyleUnit::Em(1.5)),));

    let interruption_item = builder
        .build(
            &ui,
            interruption,
            ROOT_FONT_SIZE,
            VIEWPORT,
            &mut tree,
            Some(interruption_item),
        )
        .ok_or("could not restore the block")?;

    println!(
        "\nAfter restoring the block and rebuilding {interruption_item:?} with font-size: 1.5em:"
    );
    print_tree(&ui, &tree, layout_root);

    Ok(())
}

fn print_tree(ui: &Ui, tree: &LayoutInputTree, root: LayoutInputKey) {
    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let Some(item) = tree.items.get(key) else {
            continue;
        };
        let indent = "  ".repeat(depth);
        println!(
            "{indent}{key:?} {:?}({:?}) children={:?} font_size={} flags={:?}",
            item.kind, item.node, item.child_layout, item.text_style.font_size, item.flags
        );

        if item.kind == LayoutInputKind::Text {
            let text = ui.prop::<Text>(item.node);

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
