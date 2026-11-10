use anyrender::{Glyph, Paint, Scene, recording::RenderCommand};
use comet_layout::{
    fragment::{FragmentItem, FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Rect as LayoutRect, Size, StyleUnit},
    },
};
use comet_paint::{
    CometPainter,
    style::{BorderFill, BorderRadius, Fill, TextColor},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use kurbo::{BezPath, PathEl, Rect, Shape};
use peniko::Color;

fn fills(scene: &Scene) -> Vec<(Rect, Color)> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::Fill(fill) => Some((fill.shape.bounding_box(), solid(&fill.brush))),
            _ => None,
        })
        .collect()
}

fn strokes(scene: &Scene) -> Vec<(Rect, f64)> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::Stroke(stroke) => {
                Some((stroke.shape.bounding_box(), stroke.style.width))
            }
            _ => None,
        })
        .collect()
}

fn glyphs(scene: &Scene) -> Vec<(&Glyph, Color)> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::GlyphRun(run) => Some(run),
            _ => None,
        })
        .flat_map(|run| run.glyphs.iter().map(|glyph| (glyph, solid(&run.brush))))
        .collect()
}

fn solid(paint: &Paint) -> Color {
    match paint {
        Paint::Solid(color) => *color,
        _ => panic!("test only uses solid paints"),
    }
}

fn assert_path_close(actual: &BezPath, expected: &BezPath) {
    assert_eq!(actual.elements().len(), expected.elements().len());

    for (actual, expected) in actual.iter().zip(expected.iter()) {
        match (actual, expected) {
            (PathEl::MoveTo(a), PathEl::MoveTo(b)) | (PathEl::LineTo(a), PathEl::LineTo(b)) => {
                assert!(a.distance(b) < 1e-9);
            }
            (PathEl::CurveTo(a, b, c), PathEl::CurveTo(d, e, f)) => {
                assert!(a.distance(d) < 1e-9);
                assert!(b.distance(e) < 1e-9);
                assert!(c.distance(f) < 1e-9);
            }
            (PathEl::ClosePath, PathEl::ClosePath) => {}
            _ => panic!("unexpected path elements: {actual:?}, {expected:?}"),
        }
    }
}

const VIEWPORT: Size<f64> = Size::new(120.0, 400.0);

fn build(ui: &Ui, node: NodeKey) -> (LayoutInputTree, FragmentTree, FragmentKey) {
    let mut inputs = LayoutInputTree::new();
    let root = LayoutInputTreeBuilder::new()
        .build(ui, &mut inputs, BuildRequest::new(node, 16.0, VIEWPORT))
        .unwrap();

    let mut fragments = FragmentTree::new();
    let fragment = FragmentTreeBuilder::new()
        .build(
            ui,
            &inputs,
            root,
            &mut fragments,
            &LayoutConstraint::new_root(VIEWPORT, 16.0),
        )
        .unwrap();
    inputs.clear_flags(root);

    (inputs, fragments, fragment)
}

#[test]
fn paints_wrapped_inline_backgrounds_and_glyphs_once_with_inherited_color() {
    let mut ui = Ui::new();
    let blue = Color::from_rgb8(20, 40, 180);
    let yellow = Color::from_rgb8(250, 240, 20);
    let root_node = ui.create(
        Node::Box,
        (
            TextColor(Paint::Solid(blue)),
            bx::Padding(LayoutRect::new_axis(StyleUnit::Px(7.0), StyleUnit::Px(9.0))),
        ),
    );
    let span = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, Fill(Paint::Solid(yellow))),
    );
    let text = ui.create(
        Node::Text,
        (Text(
            "one two three four five six seven eight nine ten".into(),
        ),),
    );

    ui.append(root_node, span);
    ui.append(span, text);

    let (inputs, mut tree, root) = build(&ui, root_node);
    let mut renderer = CometPainter::new();
    let mut scene = Scene::new();

    renderer.draw(&ui, &inputs, &tree, root, &mut scene);

    let mut pending = vec![(root, 0.0, 0.0)];
    let mut expected_fills = Vec::new();
    let mut expected_glyphs = 0;

    while let Some((key, parent_x, parent_y)) = pending.pop() {
        let fragment = &tree.items[key];
        let x = parent_x + fragment.layout.location.x;
        let y = parent_y + fragment.layout.location.y;

        if inputs.items[fragment.source].node == span
            && matches!(fragment.item, FragmentItem::Inline { .. })
        {
            expected_fills.push((
                Rect::new(
                    x,
                    y,
                    x + fragment.layout.size.width,
                    y + fragment.layout.size.height,
                ),
                yellow,
            ));
        }

        if let FragmentItem::Line { line } = fragment.item {
            for item in tree
                .inline_layout(key)
                .unwrap()
                .layout
                .get(line)
                .unwrap()
                .items()
            {
                if let parley::PositionedLayoutItem::GlyphRun(run) = item {
                    expected_glyphs += run.positioned_glyphs().count();
                }
            }
        }

        pending.extend(tree.items.children(key).rev().map(|child| (child, x, y)));
    }

    let scene_fills = fills(&scene);
    let scene_glyphs = glyphs(&scene);

    assert!(expected_fills.len() > 1);
    assert!(expected_glyphs > 0);
    assert_eq!(scene_fills, expected_fills);
    assert_eq!(scene_glyphs.len(), expected_glyphs);
    assert!(scene_glyphs.iter().all(|(_, color)| *color == blue));
    assert!(
        scene_glyphs
            .iter()
            .all(|(glyph, _)| glyph.x >= 9.0 && glyph.y >= 7.0)
    );

    tree.items[root].layout.location.x += 13.0;
    tree.items[root].layout.location.y += 17.0;

    let mut shifted = Scene::new();

    renderer.draw(&ui, &inputs, &tree, root, &mut shifted);

    let shifted_glyphs = glyphs(&shifted);

    assert_eq!(scene_glyphs.len(), shifted_glyphs.len());

    for ((before, _), (after, _)) in scene_glyphs.iter().zip(&shifted_glyphs) {
        assert_eq!(before.id, after.id);
        assert!((after.x - before.x - 13.0).abs() < 0.001);
        assert!((after.y - before.y - 17.0).abs() < 0.001);
    }
}

#[test]
fn paints_atomic_inline_subtree_border_and_text_override_without_double_offsets() {
    let mut ui = Ui::new();
    let red = Color::from_rgb8(180, 20, 20);
    let root_node = ui.create(Node::Box, (TextColor(Paint::Solid(Color::BLACK)),));
    let before = ui.create(Node::Text, (Text("prefix ".into()),));
    let atomic = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            bx::DisplayInner::FlowRoot,
            bx::Size(Size::new(
                AutoValue::Value(StyleUnit::Px(50.0)),
                AutoValue::Value(StyleUnit::Px(40.0)),
            )),
            bx::Border(LayoutRect::new_all(StyleUnit::Px(2.0))),
            BorderFill(Paint::Solid(Color::BLACK)),
            Fill(Paint::Solid(Color::WHITE)),
            TextColor(Paint::Solid(red)),
        ),
    );
    let text = ui.create(Node::Text, (Text("atom".into()),));

    ui.append(root_node, before);
    ui.append(root_node, atomic);
    ui.append(atomic, text);

    let (inputs, tree, root) = build(&ui, root_node);
    let mut scene = Scene::new();

    CometPainter::new().draw(&ui, &inputs, &tree, root, &mut scene);

    let scene_fills = fills(&scene);
    let scene_strokes = strokes(&scene);
    let scene_glyphs = glyphs(&scene);

    assert_eq!(scene_fills.len(), 1);
    assert_eq!(scene_strokes.len(), 1);

    let rect = scene_fills[0].0;

    assert!(rect.x0 > 0.0);
    assert_eq!(rect.width(), 50.0);
    assert_eq!(scene_strokes[0], (rect.inset(-1.0), 2.0));

    let atomic_glyphs: Vec<_> = scene_glyphs
        .iter()
        .filter(|(_, color)| *color == red)
        .collect();

    assert!(!atomic_glyphs.is_empty());
    assert!(
        atomic_glyphs
            .iter()
            .all(|(glyph, _)| glyph.x as f64 >= rect.x0
                && (glyph.x as f64) < rect.x1
                && glyph.y as f64 >= rect.y0
                && (glyph.y as f64) < rect.y1)
    );
}

#[test]
fn standalone_text_paints_its_background_only_once() {
    let mut ui = Ui::new();
    let node = ui.create(
        Node::Text,
        (Text("text".into()), Fill(Paint::Solid(Color::WHITE))),
    );

    let (inputs, tree, root) = build(&ui, node);
    let mut scene = Scene::new();

    CometPainter::new().draw(&ui, &inputs, &tree, root, &mut scene);

    assert_eq!(fills(&scene).len(), 1);
    assert!(!glyphs(&scene).is_empty());
}

#[test]
fn rounds_background_and_border_corners() {
    let mut ui = Ui::new();
    let node = ui.create(
        Node::Box,
        (
            bx::Size(Size::new(
                AutoValue::Value(StyleUnit::Px(80.0)),
                AutoValue::Value(StyleUnit::Px(60.0)),
            )),
            bx::Border(LayoutRect::new_all(StyleUnit::Px(2.0))),
            Fill(Paint::Solid(Color::WHITE)),
            BorderFill(Paint::Solid(Color::BLACK)),
            BorderRadius(LayoutRect::new_invidiual(
                StyleUnit::Px(3.0),
                StyleUnit::Em(0.25),
                StyleUnit::Rem(0.5),
                StyleUnit::Vw(0.1),
            )),
        ),
    );

    let (inputs, tree, root) = build(&ui, node);
    let mut scene = Scene::new();

    CometPainter::new().draw(&ui, &inputs, &tree, root, &mut scene);

    let [RenderCommand::Fill(fill), RenderCommand::Stroke(stroke)] = scene.commands.as_slice()
    else {
        panic!("expected a background and border");
    };
    let rect = fill.shape.bounding_box();
    let radii = (3.0, 4.0, 8.0, 12.0);

    assert_path_close(
        &fill.shape,
        &rect.to_rounded_rect(radii).to_path(scene.tolerance),
    );
    assert_path_close(
        &stroke.shape,
        &rect
            .inset(-1.0)
            .to_rounded_rect(radii)
            .to_path(scene.tolerance),
    );
}

#[test]
fn rounds_only_outer_edges_of_wrapped_fragments() {
    for standalone in [false, true] {
        let mut ui = Ui::new();
        let text = ui.create(
            Node::Text,
            (Text(
                "one two three four five six seven eight nine ten eleven twelve thirteen fourteen"
                    .into(),
            ),),
        );
        let node = if standalone {
            text
        } else {
            let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));

            ui.append(span, text);

            span
        };

        ui.set_props(
            node,
            (
                Fill(Paint::Solid(Color::WHITE)),
                BorderRadius(LayoutRect::new_all(StyleUnit::Px(3.0))),
            ),
        );

        let root_node = if standalone {
            node
        } else {
            let root = ui.create(Node::Box, ());

            ui.append(root, node);

            root
        };
        let (inputs, tree, root) = build(&ui, root_node);
        let mut scene = Scene::new();

        CometPainter::new().draw(&ui, &inputs, &tree, root, &mut scene);

        let fills: Vec<_> = scene
            .commands
            .iter()
            .filter_map(|command| match command {
                RenderCommand::Fill(fill) => Some(fill),
                _ => None,
            })
            .collect();

        assert!(fills.len() >= 3);

        for (index, fill) in fills.iter().enumerate() {
            let left = if index == 0 { 3.0 } else { 0.0 };
            let right = if index == fills.len() - 1 { 3.0 } else { 0.0 };
            let rect = fill.shape.bounding_box();

            assert_path_close(
                &fill.shape,
                &rect
                    .to_rounded_rect((left, right, right, left))
                    .to_path(scene.tolerance),
            );
        }
    }
}

#[test]
fn anonymous_blocks_paint_nothing_and_offset_their_lines() {
    let mut ui = Ui::new();
    let gray = Color::from_rgb8(200, 200, 200);
    let yellow = Color::from_rgb8(250, 240, 20);
    let root_node = ui.create(Node::Box, (Fill(Paint::Solid(gray)),));
    let span = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, Fill(Paint::Solid(yellow))),
    );
    let before = ui.create(Node::Text, (Text("before".into()),));
    let block = ui.create(
        Node::Box,
        (
            bx::Size(Size::new(
                AutoValue::Auto,
                AutoValue::Value(StyleUnit::Px(20.0)),
            )),
            Fill(Paint::Solid(Color::WHITE)),
        ),
    );
    let after = ui.create(Node::Text, (Text("after".into()),));

    ui.append(root_node, span);
    ui.append(span, before);
    ui.append(span, block);
    ui.append(span, after);

    let (inputs, tree, root) = build(&ui, root_node);
    let mut scene = Scene::new();

    CometPainter::new().draw(&ui, &inputs, &tree, root, &mut scene);

    let scene_fills = fills(&scene);
    let block_rect = scene_fills
        .iter()
        .find(|(_, color)| *color == Color::WHITE)
        .unwrap()
        .0;

    // The root, both pieces of the span and the block; anonymous blocks add none.
    assert_eq!(scene_fills.len(), 4);
    assert_eq!(
        scene_fills[0],
        (
            Rect::new(0.0, 0.0, 120.0, tree.items[root].layout.size.height),
            gray
        )
    );
    assert_eq!(
        scene_fills
            .iter()
            .filter(|(_, color)| *color == yellow)
            .count(),
        2
    );

    // Glyphs of the second run are placed below the block.
    let scene_glyphs = glyphs(&scene);
    let (above, below): (Vec<_>, Vec<_>) = scene_glyphs
        .iter()
        .partition(|(glyph, _)| (glyph.y as f64) < block_rect.y0);

    assert!(!above.is_empty());
    assert!(!below.is_empty());
    assert!(
        below
            .iter()
            .all(|(glyph, _)| glyph.y as f64 > block_rect.y1)
    );
}
