mod common;

use comet_layout::{
    fragment::{FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
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
    let mut input_builder = LayoutInputTreeBuilder::new();
    let mut fragments = FragmentTree::new();
    let mut fragment_builder = FragmentTreeBuilder::new();

    let mut viewport = Size::new(320.0, 600.0);
    let mut constraint = LayoutConstraint::new_root(viewport, ROOT_FONT_SIZE);
    let input_root = input_builder
        .build(
            &ui,
            &mut inputs,
            BuildRequest::new(root, ROOT_FONT_SIZE, viewport),
        )
        .ok_or("could not build sample UI")?;
    let fragment_root = fragment_builder
        .build(&ui, &inputs, input_root, &mut fragments, &constraint)
        .ok_or("could not build fragments")?;
    // Flags are consumed by the fragment builder and cleared by the caller.
    inputs.clear_flags(input_root);

    println!("Initial fragment tree at 320px:");
    print_tree(&inputs, &fragments, fragment_root);

    ui.set_props(
        greeting,
        (Text(
            "Updated greeting with enough text to wrap over several lines. ".into(),
        ),),
    );
    inputs.mark_node_dirty(greeting);

    let updated = fragment_builder
        .build(&ui, &inputs, input_root, &mut fragments, &constraint)
        .ok_or("could not update fragments")?;
    inputs.clear_flags(input_root);
    assert_eq!(updated, fragment_root);

    println!("\nAfter changing the greeting (box fragments retained):");
    print_tree(&inputs, &fragments, updated);

    // Viewport units may depend on the viewport, so every item is rebuilt.
    viewport = Size::new(180.0, 600.0);
    constraint = LayoutConstraint::new_root(viewport, ROOT_FONT_SIZE);
    input_builder
        .build(
            &ui,
            &mut inputs,
            BuildRequest::new(root, ROOT_FONT_SIZE, viewport).with_force(true),
        )
        .ok_or("could not rebuild sample UI")?;
    let resized = fragment_builder
        .build(&ui, &inputs, input_root, &mut fragments, &constraint)
        .ok_or("could not reflow fragments")?;
    inputs.clear_flags(input_root);

    println!("\nAfter narrowing the viewport to 180px:");
    print_tree(&inputs, &fragments, resized);

    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    inputs.mark_node_dirty(interruption);

    input_builder
        .build(
            &ui,
            &mut inputs,
            BuildRequest::new(root, ROOT_FONT_SIZE, viewport),
        )
        .ok_or("could not merge inline span")?;
    let merged = fragment_builder
        .build(&ui, &inputs, input_root, &mut fragments, &constraint)
        .ok_or("could not rebuild merged fragments")?;
    inputs.clear_flags(input_root);

    println!("\nAfter changing the interruption to inline-block:");
    print_tree(&inputs, &fragments, merged);

    Ok(())
}

fn print_tree(inputs: &LayoutInputTree, tree: &FragmentTree, root: FragmentKey) {
    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let fragment = &tree.items[key];
        let indent = "  ".repeat(depth);
        println!(
            "{indent}{key:?} {:?} node={:?} at=({:.1}, {:.1}) size=({:.1}, {:.1}) baseline={:?}",
            fragment.item,
            inputs.items.get(fragment.source).map(|item| item.node),
            fragment.layout.location.x,
            fragment.layout.location.y,
            fragment.layout.size.width,
            fragment.layout.size.height,
            fragment.layout.baseline
        );

        if fragment.link.prev.is_some() || fragment.link.next.is_some() {
            println!(
                "{indent}  prev={:?} next={:?}",
                fragment.link.prev, fragment.link.next
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
