mod common;

use comet_geometry::{Size, StyleUnit};
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    input::{
        LayoutInputKey, LayoutInputKind,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::bx::{DisplayInner, DisplayOuter, FontSize},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let layout_root = build(
        &mut builder,
        &ui,
        &mut tree,
        &mut dirty,
        root,
        ROOT_FONT_SIZE,
    )?;

    println!("Initial layout input tree (every item is flagged):");
    print_tree(&ui, &tree, &dirty, layout_root);

    // Building again only resolves the changes marked with input flags.
    // Keys are kept.

    // Text content is read by the fragment builder, not resolved into the
    // item, so only the layout flag is set and the input build skips it.
    ui.set_props(greeting, (Text("Updated greeting! ".into()),));
    dirty.clear(&tree, layout_root);
    dirty.mark_node(&tree, greeting, DirtyFlags::NEED_FULL_LAYOUT);
    build(
        &mut builder,
        &ui,
        &mut tree,
        &mut dirty,
        root,
        ROOT_FONT_SIZE,
    )?;

    println!(
        "
After changing the greeting text:"
    );
    print_tree(&ui, &tree, &dirty, layout_root);

    // An inline-block becomes an atomic inline item with its own flow. Items
    // only describe their own node, so marking the changed node is enough.
    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    dirty.clear(&tree, layout_root);
    dirty.mark_node(&tree, interruption, DirtyFlags::NEED_FULL_LAYOUT);
    build(
        &mut builder,
        &ui,
        &mut tree,
        &mut dirty,
        root,
        ROOT_FONT_SIZE,
    )?;

    println!(
        "
After changing the interruption to inline-block:"
    );
    print_tree(&ui, &tree, &dirty, layout_root);

    // Text styles are resolved during building and inherited from the parent
    // item. Descendants are resolved again while their styles change.
    ui.set_props(interruption, (DisplayOuter::Block, DisplayInner::Flow));
    ui.set_props(interruption, (FontSize(StyleUnit::Em(1.5)),));
    dirty.clear(&tree, layout_root);
    dirty.mark_node(&tree, interruption, DirtyFlags::NEED_FULL_LAYOUT);
    build(
        &mut builder,
        &ui,
        &mut tree,
        &mut dirty,
        root,
        ROOT_FONT_SIZE,
    )?;

    println!(
        "
After restoring the block with font-size: 1.5em:"
    );
    print_tree(&ui, &tree, &dirty, layout_root);

    // Inserting a node marks its parent's child list.
    let note = ui.create(Node::Text, (Text("A new note.".into()),));
    ui.append(root, note);
    dirty.clear(&tree, layout_root);
    dirty.mark_node(&tree, root, DirtyFlags::NEED_CHILDREN);
    build(
        &mut builder,
        &ui,
        &mut tree,
        &mut dirty,
        root,
        ROOT_FONT_SIZE,
    )?;

    println!(
        "
After appending a note:"
    );
    print_tree(&ui, &tree, &dirty, layout_root);

    // A new root font size is not tracked, so the tree is built from scratch.
    tree.clear();
    dirty = DirtyMap::new();
    let layout_root = build(&mut builder, &ui, &mut tree, &mut dirty, root, 20.0)?;

    println!(
        "
After rebuilding with a 20px root font size:"
    );
    print_tree(&ui, &tree, &dirty, layout_root);

    Ok(())
}

fn build(
    builder: &mut LayoutInputTreeBuilder,
    ui: &Ui,
    tree: &mut LayoutInputTree,
    dirty: &mut DirtyMap,
    root: NodeKey,
    root_font_size: f64,
) -> Result<LayoutInputKey, &'static str> {
    builder
        .prepare()
        .ui(ui)
        .tree(tree)
        .dirty(dirty)
        .request(BuildRequest::new(root, root_font_size, VIEWPORT))
        .build()
        .ok_or("could not build sample UI")
}

fn print_tree(ui: &Ui, tree: &LayoutInputTree, dirty: &DirtyMap, root: LayoutInputKey) {
    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let Some(item) = tree.items.get(key) else {
            continue;
        };
        let indent = "  ".repeat(depth);
        println!(
            "{indent}{key:?} {:?}({:?}) children={:?} font_size={} flags={:?}",
            item.kind,
            item.node,
            item.child_layout,
            item.text_style.span.font_size,
            dirty.entry(key).flags
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
