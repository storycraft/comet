mod common;

use comet_layout::{
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::geometry::Size,
};
use comet_tree::node::Text;

const VIEWPORT: Size<f64> = Size::new(640.0, 920.0);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (mut ui, root) = common::sample_ui();
    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let request = BuildRequest::new(root, 16.0, VIEWPORT);
    let input_root = builder
        .build(&ui, &mut tree, request)
        .ok_or("could not build layout input tree")?;

    println!("Initial layout input tree:");
    print_tree(&tree, input_root);
    tree.clear_flags(input_root);

    let mut text = root;
    while let Some(child) = ui.last_child(text) {
        text = child;
    }
    ui.prop::<Text>(text).ok_or("missing sample text")?;
    ui.set_props(text, (Text("Updated text".into()),));
    tree.mark_node_dirty(text);

    let updated = builder
        .build(&ui, &mut tree, request)
        .ok_or("could not update layout input tree")?;
    assert_eq!(input_root, updated);

    println!("\nAfter updating text (flags mark the path to the change):");
    print_tree(&tree, updated);

    Ok(())
}

fn print_tree(tree: &LayoutInputTree, root: LayoutInputKey) {
    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let item = &tree.items[key];
        println!(
            "{} {key:?} {:?} node={:?} children={:?} font_size={} flags={:?}",
            "  ".repeat(depth),
            item.kind,
            item.node,
            item.child_layout,
            item.text_style.font_size,
            item.flags
        );

        pending.extend(
            tree.items
                .children(key)
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }
}
