mod common;

use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::geometry::Size,
};
use comet_tree::{
    node::{NodeKey, Text},
    ui::Ui,
};

const VIEWPORT: Size<f64> = Size::new(640.0, 920.0);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sample = comet::UiRoot::new(common::app);
    let root = sample.root();
    let ui = &mut sample.tree().borrow_mut().ui;
    let mut tree = LayoutInputTree::new();
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let request = BuildRequest::new(root, 16.0, VIEWPORT);
    let input_root = builder
        .build(ui, &mut tree, &mut dirty, request)
        .ok_or("could not build layout input tree")?;

    println!("Initial layout input tree:");
    print_tree(&tree, &dirty, input_root);
    dirty.clear(&tree, input_root);

    let text = last_text(ui, root).ok_or("missing sample text")?;
    ui.set_props(text, (Text("Updated text".into()),));
    dirty.mark_node(&tree, text, DirtyFlags::NEED_FULL_LAYOUT);

    let updated = builder
        .build(ui, &mut tree, &mut dirty, request)
        .ok_or("could not update layout input tree")?;
    assert_eq!(input_root, updated);

    println!("\nAfter updating text (flags mark the path to the change):");
    print_tree(&tree, &dirty, updated);

    Ok(())
}

fn print_tree(tree: &LayoutInputTree, dirty: &DirtyMap, root: LayoutInputKey) {
    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let item = &tree.items[key];
        println!(
            "{} {key:?} {:?} node={:?} children={:?} font_size={} flags={:?}",
            "  ".repeat(depth),
            item.kind,
            item.node,
            item.child_layout,
            item.text_style.span.font_size,
            dirty.entry(key).flags
        );

        pending.extend(
            tree.items
                .children(key)
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }
}

/// The last text node under `node`, in tree order.
fn last_text(ui: &Ui, node: NodeKey) -> Option<NodeKey> {
    if ui.prop::<Text>(node).is_some() {
        return Some(node);
    }

    let children: Vec<_> = ui.children(node).collect();
    children
        .into_iter()
        .rev()
        .find_map(|child| last_text(ui, child))
}
