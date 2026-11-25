mod common;

use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{BoxFragmentKey, FragmentTree, FragmentTreeBuilder, InlineItemKind, InlineLayout},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint,
        bx::{DisplayInner, DisplayOuter},
        geometry::Size,
    },
};
use comet_tree::node::Text;

const ROOT_FONT_SIZE: f64 = 16.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let common::SampleUi {
        mut ui,
        root,
        greeting,
        interruption,
    } = common::sample_ui();
    let mut inputs = LayoutInputTree::new();
    let mut dirty = DirtyMap::new();
    let mut input_builder = LayoutInputTreeBuilder::new();
    let mut fragments = FragmentTree::new();
    let mut fragment_builder = FragmentTreeBuilder::new();

    let mut viewport = Size::new(320.0, 600.0);
    let mut constraint = LayoutConstraint::new_root(viewport, ROOT_FONT_SIZE);
    let input_root = input_builder
        .build(
            &ui,
            &mut inputs,
            &mut dirty,
            BuildRequest::new(root, ROOT_FONT_SIZE, viewport),
        )
        .ok_or("could not build sample UI")?;
    let fragment_root = fragment_builder
        .build(
            &ui,
            &inputs,
            &mut dirty,
            input_root,
            &mut fragments,
            &constraint,
        )
        .ok_or("could not build fragments")?;
    // Flags are consumed by the fragment builder and cleared by the caller.
    dirty.clear(&inputs, input_root);

    println!("Initial fragment tree at 320px:");
    print_tree(&inputs, &fragments, fragment_root);

    ui.set_props(
        greeting,
        (Text(
            "Updated greeting with enough text to wrap over several lines. ".into(),
        ),),
    );
    dirty.mark_node(&inputs, greeting, DirtyFlags::NEED_FULL_LAYOUT);

    let updated = fragment_builder
        .build(
            &ui,
            &inputs,
            &mut dirty,
            input_root,
            &mut fragments,
            &constraint,
        )
        .ok_or("could not update fragments")?;
    dirty.clear(&inputs, input_root);
    assert_eq!(updated, fragment_root);

    println!("\nAfter changing the greeting (box fragments retained):");
    print_tree(&inputs, &fragments, updated);

    // A narrower viewport reflows the lines. Only text styles depending on
    // the viewport are resolved again.
    viewport = Size::new(180.0, 600.0);
    constraint = LayoutConstraint::new_root(viewport, ROOT_FONT_SIZE).with_viewport_changed(true);
    inputs.invalidate_viewport(&mut dirty, input_root);
    input_builder
        .build(
            &ui,
            &mut inputs,
            &mut dirty,
            BuildRequest::new(root, ROOT_FONT_SIZE, viewport),
        )
        .ok_or("could not rebuild sample UI")?;
    let resized = fragment_builder
        .build(
            &ui,
            &inputs,
            &mut dirty,
            input_root,
            &mut fragments,
            &constraint,
        )
        .ok_or("could not reflow fragments")?;
    dirty.clear(&inputs, input_root);

    println!("\nAfter narrowing the viewport to 180px:");
    print_tree(&inputs, &fragments, resized);

    // Display is resolved into the input item, so the input builder resolves
    // the marked item again and flags its new kind for the fragment builder.
    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    dirty.mark_node(&inputs, interruption, DirtyFlags::NEED_FULL_LAYOUT);
    constraint = LayoutConstraint::new_root(viewport, ROOT_FONT_SIZE);

    input_builder
        .build(
            &ui,
            &mut inputs,
            &mut dirty,
            BuildRequest::new(root, ROOT_FONT_SIZE, viewport),
        )
        .ok_or("could not merge inline span")?;
    let merged = fragment_builder
        .build(
            &ui,
            &inputs,
            &mut dirty,
            input_root,
            &mut fragments,
            &constraint,
        )
        .ok_or("could not rebuild merged fragments")?;
    dirty.clear(&inputs, input_root);

    println!("\nAfter changing the interruption to inline-block:");
    print_tree(&inputs, &fragments, merged);

    Ok(())
}

fn print_tree(inputs: &LayoutInputTree, tree: &FragmentTree, root: BoxFragmentKey) {
    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let fragment = &tree.boxes[key];
        let geometry = &fragment.layout.geometry;
        let indent = "  ".repeat(depth);
        println!(
            "{indent}{key:?} {:?} node={:?} at=({:.1}, {:.1}) size=({:.1}, {:.1}) baseline={:?}",
            fragment.box_type,
            inputs.items.get(fragment.source).map(|item| item.node),
            geometry.location.x,
            geometry.location.y,
            geometry.size.width,
            geometry.size.height,
            fragment.layout.baseline
        );

        if fragment.link.prev.is_some() || fragment.link.next.is_some() {
            println!(
                "{indent}  prev={:?} next={:?}",
                fragment.link.prev, fragment.link.next
            );
        }

        if let Some(inline) = fragment.inline() {
            for line in inline.roots() {
                print_inline_item(inputs, inline, line, depth + 1);
            }
        }

        pending.extend(
            tree.boxes
                .children(key)
                .rev()
                .map(|child| (child, depth + 1)),
        );
    }
}

/// Print an item of an inline layout and its subtree.
fn print_inline_item(inputs: &LayoutInputTree, inline: &InlineLayout, index: usize, depth: usize) {
    let item = &inline.items[index];
    let geometry = &item.geometry;
    let kind = match item.kind {
        InlineItemKind::Line { line } => format!("Line({line})"),
        InlineItemKind::Box { child } => format!("Box({child:?})"),
        InlineItemKind::Run { line, run } => format!("Run({line}, {run})"),
    };
    println!(
        "{}#{index} {kind} node={:?} at=({:.1}, {:.1}) size=({:.1}, {:.1})",
        "  ".repeat(depth),
        inputs.items.get(item.source).map(|item| item.node),
        geometry.location.x,
        geometry.location.y,
        geometry.size.width,
        geometry.size.height,
    );

    for child in inline.children(index) {
        print_inline_item(inputs, inline, child, depth + 1);
    }
}
