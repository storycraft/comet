mod common;

use comet_layout::{
    style::bx::{DisplayInner, DisplayOuter},
    tree::{LayoutItemKey, LayoutItemTy, LayoutTree, builder::LayoutTreeBuilder},
};
use comet_tree::{node::Text, ui::Ui};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let common::SampleUi {
        mut ui,
        root,
        greeting,
        interruption,
    } = common::sample_ui();

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let layout_root = builder
        .build(&ui, root, &mut tree, None)
        .ok_or("could not build sample UI")?;

    println!("Initial layout tree with a split inline span:");
    print_tree(&ui, &tree, layout_root);

    // Text stays in the UI; updating preserves compatible layout object keys.
    ui.set_props(greeting, (Text("Updated greeting! ".into()),));
    tree.mark_node_dirty(greeting);

    let layout_root = builder
        .build(&ui, root, &mut tree, Some(layout_root))
        .ok_or("could not update sample UI")?;

    println!("\nAfter changing the greeting:");
    print_tree(&ui, &tree, layout_root);

    // An atomic inline no longer interrupts the span: its parts merge and unlink.
    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    tree.mark_node_dirty(interruption);

    let layout_root = builder
        .build(&ui, root, &mut tree, Some(layout_root))
        .ok_or("could not merge the inline span")?;

    println!("\nAfter changing the interruption to inline-block (merged span):");
    print_tree(&ui, &tree, layout_root);

    // Restoring the block splits the span and rebuilds its continuation chain.
    ui.set_props(interruption, (DisplayOuter::Block, DisplayInner::Flow));
    tree.mark_node_dirty(interruption);

    let layout_root = builder
        .build(&ui, root, &mut tree, Some(layout_root))
        .ok_or("could not split the inline span again")?;

    println!("\nAfter restoring the block (split span):");
    print_tree(&ui, &tree, layout_root);

    Ok(())
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

        if let LayoutItemTy::Inline(_) = item.ty {
            let part = match (item.link.prev, item.link.next) {
                (None, None) => "unsplit",
                (None, Some(_)) => "first",
                (Some(_), Some(_)) => "middle",
                (Some(_), None) => "last",
            };

            println!(
                "{indent}  part={part} prev={:?} next={:?}",
                item.link.prev, item.link.next
            );
        }

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
