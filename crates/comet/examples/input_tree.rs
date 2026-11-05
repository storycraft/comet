mod common;

use comet_layout::tree::{LayoutItemKey, LayoutTree, builder::LayoutTreeBuilder};
use comet_tree::node::Text;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut ui, root) = common::sample_ui();
    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let layout_root = builder
        .build(&ui, root, &mut tree, None)
        .ok_or("could not build layout tree")?;

    println!("Initial layout tree:");
    print_tree(&tree, layout_root);

    let mut text = root;
    while let Some(child) = ui.last_child(text) {
        text = child;
    }
    ui.prop::<Text>(text).ok_or("missing sample text")?;
    ui.set_props(text, (Text("Updated text".into()),));
    tree.mark_dirty(layout_root);

    let updated = builder
        .build(&ui, root, &mut tree, Some(layout_root))
        .ok_or("could not update layout tree")?;
    assert_eq!(layout_root, updated);

    println!("\nAfter updating text:");
    print_tree(&tree, updated);

    Ok(())
}

fn print_tree(tree: &LayoutTree, root: LayoutItemKey) {
    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let item = &tree.items[key];
        println!(
            "{} {key:?} {:?} children={:?}",
            "  ".repeat(depth),
            item.ty,
            item.child_layout
        );

        pending.extend(
            tree.items
                .children(key)
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }
}
