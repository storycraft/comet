//! Boxes displaying a content, sized by it.

use comet_content::{
    Content, DrawInput, MeasureInput, manager::ContentManager, style::ObjectStyle,
};
use comet_geometry::{AutoValue, Rect, Size, StyleUnit};
use comet_layout::{
    dirty_map::DirtyMap,
    fragment::{builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint,
        bx::{self, DisplayInner},
    },
};
use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};

const VIEWPORT: Size<f64> = Size::new(400.0, 300.0);

/// A content 30 by 15 in size, keeping its aspect ratio.
struct Natural;

impl Content for Natural {
    fn measure(&self, input: &MeasureInput<'_>) -> taffy::Size<f32> {
        ObjectStyle::default().measure(kurbo::Size::new(30.0, 15.0), input)
    }

    fn draw(&self, _: &DrawInput<'_>, _: &mut anyrender::Scene) {}
}

/// Lay out `root` with `contents` and return the border box size of each of
/// `nodes`.
fn sizes(ui: &Ui, root: NodeKey, contents: &ContentManager, nodes: &[NodeKey]) -> Vec<(f64, f64)> {
    let mut inputs = LayoutInputTree::new();
    let mut dirty = DirtyMap::new();
    let input_root = LayoutInputTreeBuilder::new()
        .build(
            ui,
            &mut inputs,
            &mut dirty,
            BuildRequest::new(root, 16.0, VIEWPORT),
        )
        .unwrap();
    let mut fragments = FragmentTree::new();
    FragmentTreeBuilder::new()
        .build(
            ui,
            &inputs,
            contents,
            &mut dirty,
            input_root,
            &mut fragments,
            &LayoutConstraint::new_root(VIEWPORT, 16.0, 1.0),
        )
        .unwrap();

    nodes
        .iter()
        .map(|&node| {
            let key = fragments
                .box_fragment(inputs.item_for_node(node).unwrap())
                .unwrap();
            let size = fragments.boxes[key].layout.geometry.size;

            (size.width, size.height)
        })
        .collect()
}

fn px(value: f64) -> AutoValue<StyleUnit> {
    AutoValue::Value(StyleUnit::Px(value))
}

#[test]
fn content_boxes_are_sized_by_their_content() {
    let mut contents = ContentManager::new();
    let id = contents.insert(Natural);

    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let natural = ui.create(Node::Box, (DisplayInner::Content(id),));
    // A known width keeps the aspect ratio.
    let wide = ui.create(
        Node::Box,
        (
            DisplayInner::Content(id),
            bx::Size(Size::new(px(60.0), AutoValue::Auto)),
        ),
    );
    // Padding and border wrap around the measured content box.
    let padded = ui.create(
        Node::Box,
        (
            DisplayInner::Content(id),
            bx::Padding(Rect::new_all(StyleUnit::Px(3.0))),
            bx::Border(Rect::new_all(StyleUnit::Px(2.0))),
        ),
    );
    for node in [natural, wide, padded] {
        ui.append(root, node);
    }

    assert_eq!(
        sizes(&ui, root, &contents, &[natural, wide, padded]),
        [(30.0, 15.0), (60.0, 30.0), (40.0, 25.0)]
    );
}

#[test]
fn missing_contents_fall_back_to_their_style() {
    let mut contents = ContentManager::new();
    let id = contents.insert(Natural);
    contents.remove(id);

    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let empty = ui.create(Node::Box, (DisplayInner::Content(id),));
    let sized = ui.create(
        Node::Box,
        (
            DisplayInner::Content(id),
            bx::Size(Size::new(px(20.0), px(10.0))),
        ),
    );
    ui.append(root, empty);
    ui.append(root, sized);

    assert_eq!(
        sizes(&ui, root, &contents, &[empty, sized]),
        [(0.0, 0.0), (20.0, 10.0)]
    );
}

/// A content 30 physical pixels square, measured in logical units.
struct Physical;

impl Content for Physical {
    fn measure(&self, input: &MeasureInput<'_>) -> taffy::Size<f32> {
        let side = (30.0 / input.scale_factor) as f32;

        taffy::Size {
            width: side,
            height: side,
        }
    }

    fn draw(&self, _: &DrawInput<'_>, _: &mut anyrender::Scene) {}
}

#[test]
fn content_boxes_are_measured_again_with_a_new_scale_factor() {
    let mut contents = ContentManager::new();
    let id = contents.insert(Physical);

    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let content = ui.create(Node::Box, (DisplayInner::Content(id),));
    ui.append(root, content);

    let mut inputs = LayoutInputTree::new();
    let mut dirty = DirtyMap::new();
    let input_root = LayoutInputTreeBuilder::new()
        .build(
            &ui,
            &mut inputs,
            &mut dirty,
            BuildRequest::new(root, 16.0, VIEWPORT),
        )
        .unwrap();
    let mut fragments = FragmentTree::new();
    let mut builder = FragmentTreeBuilder::new();
    let mut layout = |dirty: &mut DirtyMap, scale_factor: f64, viewport_changed: bool| {
        builder
            .build(
                &ui,
                &inputs,
                &contents,
                dirty,
                input_root,
                &mut fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0, scale_factor)
                    .with_viewport_changed(viewport_changed),
            )
            .unwrap();
        dirty.clear(&inputs, input_root);

        let key = fragments
            .box_fragment(inputs.item_for_node(content).unwrap())
            .unwrap();
        let size = fragments.boxes[key].layout.geometry.size;

        (size.width, size.height)
    };

    assert_eq!(layout(&mut dirty, 1.0, false), (30.0, 30.0));
    // A display twice as dense halves the logical size.
    assert_eq!(layout(&mut dirty, 2.0, true), (15.0, 15.0));
}
