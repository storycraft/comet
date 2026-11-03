mod common;

use comet_layout2::{
    fragment::{FragmentKey, builder::FragmentBuilder, tree::FragmentTree},
    style::{
        LayoutConstraint,
        bx::{DisplayInner, DisplayOuter},
        geometry::Size,
    },
    tree::{LayoutTree, builder::LayoutTreeBuilder},
};
use comet_tree::node::Text;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let common::SampleUi {
        mut ui,
        root,
        greeting,
        interruption,
    } = common::sample_ui();
    let mut layout = LayoutTree::new();
    let mut layout_builder = LayoutTreeBuilder::new();
    let mut fragments = FragmentTree::new();
    let mut fragment_builder = FragmentBuilder::new();
    let mut constraint = LayoutConstraint::new_root(Size::new(320.0, 600.0), 16.0);
    let mut layout_root = layout_builder
        .build(&ui, root, &mut layout, None)
        .ok_or("could not build sample UI")?;
    let fragment_root = fragment_builder
        .build(&constraint, &ui, &layout, layout_root, &mut fragments)
        .ok_or("could not build fragments")?;

    println!("Initial fragment tree at 320px:");
    print_tree(&fragments, fragment_root);

    ui.set_props(
        greeting,
        (Text(
            "Updated greeting with enough text to wrap over several lines. ".into(),
        ),),
    );
    layout_root = layout_builder
        .build(&ui, root, &mut layout, Some(layout_root))
        .ok_or("could not update sample UI")?;
    let updated = fragment_builder
        .build(&constraint, &ui, &layout, layout_root, &mut fragments)
        .ok_or("could not update fragments")?;
    assert_eq!(updated, fragment_root);

    println!("\nAfter changing the greeting (root slot retained):");
    print_tree(&fragments, updated);

    constraint = LayoutConstraint::new_root(Size::new(180.0, 600.0), 16.0);
    let resized = fragment_builder
        .build(&constraint, &ui, &layout, layout_root, &mut fragments)
        .ok_or("could not reflow fragments")?;

    println!("\nAfter narrowing the viewport to 180px:");
    print_tree(&fragments, resized);

    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    layout_root = layout_builder
        .build(&ui, root, &mut layout, Some(layout_root))
        .ok_or("could not merge inline span")?;
    let merged = fragment_builder
        .build(&constraint, &ui, &layout, layout_root, &mut fragments)
        .ok_or("could not rebuild merged fragments")?;

    println!("\nAfter changing the interruption to inline-block:");
    print_tree(&fragments, merged);

    Ok(())
}

fn print_tree(tree: &FragmentTree, root: FragmentKey) {
    let mut pending = vec![(root, 0)];

    while let Some((key, depth)) = pending.pop() {
        let fragment = &tree.items[key];
        let indent = "  ".repeat(depth);
        println!(
            "{indent}{key:?} {:?} node={:?} at=({:.1}, {:.1}) size=({:.1}, {:.1}) baseline={:?}",
            fragment.item,
            fragment.node,
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
