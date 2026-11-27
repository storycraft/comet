//! Build the accessibility tree of a small UI and print it as
//! `accesskit_consumer`, which platform adapters are built on, reads it:
//! first every node, then the nodes adapters expose.

use accesskit::{NodeId, Role as AxRole, TreeId, TreeInfo, TreeUpdate};
use accesskit_consumer::{NodeRef, Tree, common_filter};
use comet_accesskit::{builder::AxTreeBuilder, node_id, style::Role, tree::AxTree};
use comet_layout::{
    dirty_map::DirtyMap,
    fragment::{builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint,
        bx::{DisplayOuter, Padding},
        geometry::{Rect, Size, StyleUnit},
    },
};
use comet_paint::{
    layer::{builder::PaintLayerBuilder, tree::PaintLayerTree},
    prepaint::{builder::PrepaintTreeBuilder, tree::PrepaintTree},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

const VIEWPORT: Size<f64> = Size::new(320.0, 240.0);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut ui = Ui::new();
    let root = sample(&mut ui);

    // Lay out the UI and build its accessibility tree.
    let mut inputs = LayoutInputTree::new();
    let mut dirty = DirtyMap::new();
    let mut fragments = FragmentTree::new();
    let mut prepaint = PrepaintTree::new();
    let mut layers = PaintLayerTree::new();
    let mut tree = AxTree::new();

    let input_root = LayoutInputTreeBuilder::new()
        .build(
            &ui,
            &mut inputs,
            &mut dirty,
            BuildRequest::new(root, 16.0, VIEWPORT),
        )
        .ok_or("could not build the layout input tree")?;
    let constraint = LayoutConstraint::new_root(VIEWPORT, 16.0);
    let fragment_root = FragmentTreeBuilder::new()
        .build(
            &ui,
            &inputs,
            &mut dirty,
            input_root,
            &mut fragments,
            &constraint,
        )
        .ok_or("could not build the fragment tree")?;
    PrepaintTreeBuilder::new()
        .build(
            &ui,
            &inputs,
            &dirty,
            &fragments,
            &constraint.units,
            &mut prepaint,
            fragment_root,
            false,
        )
        .ok_or("could not resolve the pre-paint tree")?;
    PaintLayerBuilder::new()
        .build(
            &ui,
            &inputs,
            &dirty,
            &fragments,
            &constraint.units,
            &mut layers,
            input_root,
            false,
        )
        .ok_or("could not build the paint layers")?;
    let root_key = AxTreeBuilder::new()
        .build(
            &ui, &inputs, &dirty, &fragments, &prepaint, &layers, &mut tree, input_root,
        )
        .ok_or("could not build the accessibility tree")?;

    // The first update holds every node.
    let root_id = node_id(root_key);
    let consumer = Tree::new(
        TreeUpdate {
            nodes: tree.take_update(),
            tree: Some(TreeInfo::new(root_id)),
            tree_id: TreeId::ROOT,
            focus: root_id,
        },
        false,
    );
    let root = consumer.state().root();

    println!("Every node:");
    print_all(&root, 0);

    println!("\nExposed by platform adapters:");
    print_exposed(&root, 0);

    Ok(())
}

/// A page with a heading, a paragraph with an inline span, and a button.
fn sample(ui: &mut Ui) -> NodeKey {
    let root = ui.create(
        Node::Box,
        (
            Role(AxRole::Main),
            Padding(Rect::new_all(StyleUnit::Px(8.0))),
        ),
    );

    let heading = ui.create(Node::Box, (Role(AxRole::Heading),));
    let title = ui.create(Node::Text, (Text("Comet accessibility".into()),));
    ui.append(heading, title);

    let paragraph = ui.create(Node::Box, ());
    let before = ui.create(Node::Text, (Text("Text with ".into()),));
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let inner = ui.create(Node::Text, (Text("an inline span".into()),));
    let after = ui.create(Node::Text, (Text(" that wraps across lines.".into()),));
    ui.append(span, inner);
    ui.append(paragraph, before);
    ui.append(paragraph, span);
    ui.append(paragraph, after);

    let button = ui.create(
        Node::Box,
        (
            Role(AxRole::Button),
            Padding(Rect::new_all(StyleUnit::Px(4.0))),
        ),
    );
    let label = ui.create(Node::Text, (Text("Press me".into()),));
    ui.append(button, label);

    ui.append(root, heading);
    ui.append(root, paragraph);
    ui.append(root, button);

    root
}

/// Print a node and its descendants, with text run values.
fn print_all(node: &NodeRef, depth: usize) {
    println!("{}{}", "  ".repeat(depth), describe(node));
    for child in node.children() {
        print_all(&child, depth + 1);
    }
}

/// Print a node and the descendants adapters expose: generic containers and
/// text runs are left out, their text reaching the labels holding them.
fn print_exposed(node: &NodeRef, depth: usize) {
    println!("{}{}", "  ".repeat(depth), describe(node));
    for child in node.filtered_children(common_filter) {
        print_exposed(&child, depth + 1);
    }
}

/// The role, id, name or value, and bounds of a node, as adapters read
/// them.
fn describe(node: &NodeRef) -> String {
    let (NodeId(id), _) = node.locate();
    let mut line = format!("{:?} #{id:x}", node.role());

    // Labels are named by their value, the text of their runs.
    let name = if node.label_comes_from_value() {
        node.value()
    } else {
        node.label()
    };
    if let Some(name) = name {
        line.push_str(&format!(" name={name:?}"));
    } else if let Some(value) = node.value() {
        line.push_str(&format!(" value={value:?}"));
    }

    // In window coordinates, through every ancestor's transform.
    if let Some(bounds) = node.bounding_box() {
        line.push_str(&format!(
            " bounds=({:.1}, {:.1}, {:.1}, {:.1})",
            bounds.x0, bounds.y0, bounds.x1, bounds.y1
        ));
    }

    line
}
