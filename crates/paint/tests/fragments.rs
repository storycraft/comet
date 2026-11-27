use anyrender::{AntiAliasing as EdgeMode, Glyph, Paint, Scene, recording::RenderCommand};
use comet_layout::{
    dirty_map::DirtyMap,
    fragment::{BoxFragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Rect as LayoutRect, Size, StyleUnit, UnitResolveContext},
    },
};
use comet_paint::style;
use comet_paint::{
    layer::{builder::PaintLayerBuilder, tree::PaintLayerTree},
    painter::CometPainter,
    prepaint::{builder::PrepaintTreeBuilder, tree::PrepaintTree},
    selection::rects::SelectionRects,
    style::{AntiAliasing, Background, BorderFill, BorderRadius},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use kurbo::{BezPath, PathEl, Point, Rect, Shape};
use peniko::Color;
use taffy::Position;

fn fills(scene: &Scene) -> Vec<(Rect, Color)> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::Fill(fill) => Some((
                fill.transform
                    .transform_rect_bbox(fill.shape.bounding_box()),
                solid(&fill.brush),
            )),
            _ => None,
        })
        .collect()
}

fn strokes(scene: &Scene) -> Vec<(Rect, f64)> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::Stroke(stroke) => Some((
                stroke
                    .transform
                    .transform_rect_bbox(stroke.shape.bounding_box()),
                stroke.style.width,
            )),
            _ => None,
        })
        .collect()
}

/// Glyphs in scene coordinates, with their colors.
fn glyphs(scene: &Scene) -> Vec<(Glyph, Color)> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::GlyphRun(run) => Some(run),
            _ => None,
        })
        .flat_map(|run| {
            run.glyphs.iter().map(|glyph| {
                let point = run.transform * Point::new(glyph.x as f64, glyph.y as f64);
                let glyph = Glyph {
                    id: glyph.id,
                    x: point.x as f32,
                    y: point.y as f32,
                };

                (glyph, solid(&run.brush))
            })
        })
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
const SCENE_VIEWPORT: Rect = Rect::new(0.0, 0.0, 120.0, 400.0);
const UNITS: UnitResolveContext = LayoutConstraint::new_root(VIEWPORT, 16.0).units;

fn build(ui: &Ui, node: NodeKey) -> (LayoutInputTree, FragmentTree, BoxFragmentKey) {
    let mut inputs = LayoutInputTree::new();
    let mut dirty = DirtyMap::new();
    let root = LayoutInputTreeBuilder::new()
        .build(
            ui,
            &mut inputs,
            &mut dirty,
            BuildRequest::new(node, 16.0, VIEWPORT),
        )
        .unwrap();

    let mut fragments = FragmentTree::new();
    let fragment = FragmentTreeBuilder::new()
        .build(
            ui,
            &inputs,
            &mut dirty,
            root,
            &mut fragments,
            &LayoutConstraint::new_root(VIEWPORT, 16.0),
        )
        .unwrap();
    // Flags are dropped: `draw` resolves into fresh trees, which resolve
    // every fragment.

    (inputs, fragments, fragment)
}

/// Resolve the pre-paint tree and build the paint layers of the fragments,
/// then paint them.
#[allow(clippy::too_many_arguments)]
fn draw(
    painter: &mut CometPainter,
    ui: &Ui,
    inputs: &LayoutInputTree,
    tree: &FragmentTree,
    root: BoxFragmentKey,
    units: &UnitResolveContext,
    viewport: Rect,
    scene: &mut Scene,
) {
    // Fresh trees resolve every fragment, so no flags are needed.
    let dirty = &mut DirtyMap::new();
    let mut prepaint = PrepaintTree::new();
    PrepaintTreeBuilder::new()
        .build(ui, inputs, dirty, tree, units, &mut prepaint, root, false)
        .unwrap();
    let mut layers = PaintLayerTree::new();
    let layer_root = PaintLayerBuilder::new()
        .build(
            ui,
            inputs,
            dirty,
            tree,
            &UNITS,
            &mut layers,
            tree.boxes[root].source,
            false,
        )
        .unwrap();
    painter.draw(
        ui,
        inputs,
        tree,
        &prepaint,
        &SelectionRects::default(),
        &layers,
        layer_root,
        viewport,
        1.0,
        scene,
    );
}

#[test]
fn paints_wrapped_inline_backgrounds_and_glyphs_once_with_inherited_color() {
    let mut ui = Ui::new();
    let blue = Color::from_rgb8(20, 40, 180);
    let yellow = Color::from_rgb8(250, 240, 20);
    let root_node = ui.create(
        Node::Box,
        (
            style::Color(AutoValue::Value(Paint::Solid(blue))),
            bx::Padding(LayoutRect::new_axis(StyleUnit::Px(7.0), StyleUnit::Px(9.0))),
        ),
    );
    let span = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, Background(Paint::Solid(yellow))),
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

    draw(
        &mut renderer,
        &ui,
        &inputs,
        &tree,
        root,
        &UNITS,
        SCENE_VIEWPORT,
        &mut scene,
    );

    // The span's inline part boxes are placed in the root's border box.
    let origin = tree.boxes[root].layout.geometry.location;
    let span_item = inputs.item_for_node(span).unwrap();
    let expected_fills: Vec<_> = tree
        .inline_boxes(&inputs, span_item)
        .map(|part| {
            let geometry = &tree.boxes[part].layout.geometry;
            let (x, y) = (
                origin.x + geometry.location.x,
                origin.y + geometry.location.y,
            );

            (
                Rect::new(x, y, x + geometry.size.width, y + geometry.size.height),
                yellow,
            )
        })
        .collect();
    let expected_glyphs: usize = tree
        .inline_layout(root)
        .unwrap()
        .layout
        .lines()
        .flat_map(|line| line.items().collect::<Vec<_>>())
        .map(|item| match item {
            parley::PositionedLayoutItem::GlyphRun(run) => run.positioned_glyphs().count(),
            parley::PositionedLayoutItem::InlineBox(_) => 0,
        })
        .sum();

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

    tree.boxes[root].layout.geometry.location.x += 13.0;
    tree.boxes[root].layout.geometry.location.y += 17.0;

    let mut shifted = Scene::new();

    draw(
        &mut renderer,
        &ui,
        &inputs,
        &tree,
        root,
        &UNITS,
        SCENE_VIEWPORT,
        &mut shifted,
    );

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
    let root_node = ui.create(
        Node::Box,
        (style::Color(AutoValue::Value(Paint::Solid(Color::BLACK))),),
    );
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
            Background(Paint::Solid(Color::WHITE)),
            style::Color(AutoValue::Value(Paint::Solid(red))),
        ),
    );
    let text = ui.create(Node::Text, (Text("atom".into()),));

    ui.append(root_node, before);
    ui.append(root_node, atomic);
    ui.append(atomic, text);

    let (inputs, tree, root) = build(&ui, root_node);
    let mut scene = Scene::new();

    draw(
        &mut CometPainter::new(),
        &ui,
        &inputs,
        &tree,
        root,
        &UNITS,
        SCENE_VIEWPORT,
        &mut scene,
    );

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
fn text_nodes_paint_no_background_of_their_own() {
    let mut ui = Ui::new();
    let node = ui.create(
        Node::Text,
        (Text("text".into()), Background(Paint::Solid(Color::WHITE))),
    );

    let (inputs, tree, root) = build(&ui, node);
    let mut scene = Scene::new();

    draw(
        &mut CometPainter::new(),
        &ui,
        &inputs,
        &tree,
        root,
        &UNITS,
        SCENE_VIEWPORT,
        &mut scene,
    );

    // Text generates no box of its own: as a root, it gets an anonymous box,
    // which paints nothing.
    assert!(fills(&scene).is_empty());
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
            Background(Paint::Solid(Color::WHITE)),
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

    draw(
        &mut CometPainter::new(),
        &ui,
        &inputs,
        &tree,
        root,
        &UNITS,
        SCENE_VIEWPORT,
        &mut scene,
    );

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
    {
        let mut ui = Ui::new();
        let text = ui.create(
            Node::Text,
            (Text(
                "one two three four five six seven eight nine ten eleven twelve thirteen fourteen"
                    .into(),
            ),),
        );
        let node = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
        ui.append(node, text);

        ui.set_props(
            node,
            (
                Background(Paint::Solid(Color::WHITE)),
                BorderRadius(LayoutRect::new_all(StyleUnit::Px(3.0))),
            ),
        );

        let root_node = ui.create(Node::Box, ());
        ui.append(root_node, node);
        let (inputs, tree, root) = build(&ui, root_node);
        let mut scene = Scene::new();

        draw(
            &mut CometPainter::new(),
            &ui,
            &inputs,
            &tree,
            root,
            &UNITS,
            SCENE_VIEWPORT,
            &mut scene,
        );

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
fn anonymous_run_boxes_paint_nothing_and_offset_their_lines() {
    let mut ui = Ui::new();
    let gray = Color::from_rgb8(200, 200, 200);
    let yellow = Color::from_rgb8(250, 240, 20);
    let root_node = ui.create(Node::Box, (Background(Paint::Solid(gray)),));
    let span = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, Background(Paint::Solid(yellow))),
    );
    let before = ui.create(Node::Text, (Text("before".into()),));
    let block = ui.create(
        Node::Box,
        (
            bx::Size(Size::new(
                AutoValue::Auto,
                AutoValue::Value(StyleUnit::Px(20.0)),
            )),
            Background(Paint::Solid(Color::WHITE)),
        ),
    );
    let after = ui.create(Node::Text, (Text("after".into()),));

    ui.append(root_node, span);
    ui.append(span, before);
    ui.append(span, block);
    ui.append(span, after);

    let (inputs, tree, root) = build(&ui, root_node);
    let mut scene = Scene::new();

    draw(
        &mut CometPainter::new(),
        &ui,
        &inputs,
        &tree,
        root,
        &UNITS,
        SCENE_VIEWPORT,
        &mut scene,
    );

    let scene_fills = fills(&scene);
    let block_rect = scene_fills
        .iter()
        .find(|(_, color)| *color == Color::WHITE)
        .unwrap()
        .0;

    // The root, both pieces of the span and the block; run boxes add none.
    assert_eq!(scene_fills.len(), 4);
    assert_eq!(
        scene_fills[0],
        (
            Rect::new(
                0.0,
                0.0,
                120.0,
                tree.boxes[root].layout.geometry.size.height
            ),
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

#[test]
fn skips_fragments_outside_viewport_but_paints_their_overflowing_children() {
    let mut ui = Ui::new();
    let red = Color::from_rgb8(200, 20, 20);
    let green = Color::from_rgb8(20, 200, 20);
    let blue = Color::from_rgb8(20, 20, 200);
    let height = |px: f64| {
        bx::Size(Size::new(
            AutoValue::Auto,
            AutoValue::Value(StyleUnit::Px(px)),
        ))
    };

    let root_node = ui.create(Node::Box, ());
    // Empty box at the top edge, overflowed by its child.
    let empty = ui.create(Node::Box, (height(0.0), Background(Paint::Solid(red))));
    let overflow = ui.create(Node::Box, (height(40.0), Background(Paint::Solid(blue))));
    // Below the overflowing child.
    let below = ui.create(
        Node::Box,
        (
            height(40.0),
            bx::Margin(LayoutRect::new_invidiual(
                AutoValue::Value(StyleUnit::Px(40.0)),
                AutoValue::Value(StyleUnit::Px(0.0)),
                AutoValue::Value(StyleUnit::Px(0.0)),
                AutoValue::Value(StyleUnit::Px(0.0)),
            )),
            Background(Paint::Solid(green)),
        ),
    );

    ui.append(root_node, empty);
    ui.append(empty, overflow);
    ui.append(root_node, below);

    let (inputs, tree, root) = build(&ui, root_node);

    let mut all = Scene::new();
    draw(
        &mut CometPainter::new(),
        &ui,
        &inputs,
        &tree,
        root,
        &UNITS,
        SCENE_VIEWPORT,
        &mut all,
    );

    assert_eq!(
        fills(&all)
            .iter()
            .map(|(_, color)| *color)
            .collect::<Vec<_>>(),
        [red, blue, green]
    );

    let mut culled = Scene::new();
    draw(
        &mut CometPainter::new(),
        &ui,
        &inputs,
        &tree,
        root,
        &UNITS,
        // Below the empty box and above the box after it.
        Rect::new(0.0, 1.0, 120.0, 39.0),
        &mut culled,
    );

    assert_eq!(fills(&culled), [(Rect::new(0.0, 0.0, 120.0, 40.0), blue)]);
}

fn px(value: f64) -> AutoValue<StyleUnit> {
    AutoValue::Value(StyleUnit::Px(value))
}

fn fixed(width: f64, height: f64) -> bx::Size {
    bx::Size(Size::new(px(width), px(height)))
}

fn inset(top: AutoValue<StyleUnit>, left: AutoValue<StyleUnit>) -> bx::Inset {
    bx::Inset(LayoutRect::new_invidiual(
        top,
        AutoValue::Auto,
        AutoValue::Auto,
        left,
    ))
}

fn paint(ui: &Ui, node: NodeKey) -> Scene {
    let (inputs, tree, root) = build(ui, node);
    let mut scene = Scene::new();
    draw(
        &mut CometPainter::new(),
        ui,
        &inputs,
        &tree,
        root,
        &UNITS,
        SCENE_VIEWPORT,
        &mut scene,
    );
    scene
}

#[test]
fn paints_descendants_moved_into_view_from_a_box_below_it() {
    let mut ui = Ui::new();
    let blue = Color::from_rgb8(20, 20, 200);
    let green = Color::from_rgb8(20, 200, 20);
    let root = ui.create(Node::Box, ());
    // Below the 400px viewport, and so are its own fills.
    let below = ui.create(
        Node::Box,
        (
            fixed(120.0, 50.0),
            bx::Margin(LayoutRect::new_invidiual(
                px(500.0),
                px(0.0),
                px(0.0),
                px(0.0),
            )),
        ),
    );
    let offscreen = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), Background(Paint::Solid(blue))),
    );
    // Moved up into view.
    let shifted = ui.create(
        Node::Box,
        (
            fixed(10.0, 10.0),
            inset(px(-300.0), AutoValue::Auto),
            Background(Paint::Solid(green)),
        ),
    );
    ui.append(root, below);
    ui.append(below, offscreen);
    ui.append(below, shifted);

    assert_eq!(
        fills(&paint(&ui, root)),
        [(Rect::new(0.0, 210.0, 10.0, 220.0), green)]
    );
}

#[test]
fn paints_hoisted_boxes_positioned_into_view_past_boxes_below_it() {
    let mut ui = Ui::new();
    let green = Color::from_rgb8(20, 200, 20);
    let root = ui.create(Node::Box, ());
    // Static, so the root positions the absolute box inside it.
    let below = ui.create(
        Node::Box,
        (
            bx::Position(Position::Static),
            fixed(120.0, 50.0),
            bx::Margin(LayoutRect::new_invidiual(
                px(500.0),
                px(0.0),
                px(0.0),
                px(0.0),
            )),
        ),
    );
    let inner = ui.create(
        Node::Box,
        (bx::Position(Position::Static), fixed(120.0, 50.0)),
    );
    let abs = ui.create(
        Node::Box,
        (
            bx::Position(Position::Absolute),
            inset(px(5.0), px(7.0)),
            fixed(10.0, 10.0),
            Background(Paint::Solid(green)),
        ),
    );
    ui.append(root, below);
    ui.append(below, inner);
    ui.append(inner, abs);

    assert_eq!(
        fills(&paint(&ui, root)),
        [(Rect::new(7.0, 5.0, 17.0, 15.0), green)]
    );
}

#[test]
fn paints_with_the_anti_aliasing_of_each_node() {
    let mut ui = Ui::new();
    let color = Paint::Solid(Color::from_rgb8(20, 20, 200));
    let root = ui.create(Node::Box, ());
    let smooth = ui.create(Node::Box, (fixed(10.0, 10.0), Background(color.clone())));
    let aliased = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), Background(color), AntiAliasing(false)),
    );
    // Not inherited: the text sets its own.
    let para = ui.create(Node::Box, (AntiAliasing(false),));
    let smooth_text = ui.create(Node::Text, (Text("a".into()),));
    let aliased_para = ui.create(Node::Box, ());
    let aliased_text = ui.create(Node::Text, (Text("b".into()), AntiAliasing(false)));
    ui.append(root, smooth);
    ui.append(root, aliased);
    ui.append(root, para);
    ui.append(para, smooth_text);
    ui.append(root, aliased_para);
    ui.append(aliased_para, aliased_text);

    let modes: Vec<_> = paint(&ui, root)
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::Fill(fill) => Some(("fill", fill.anti_aliasing)),
            RenderCommand::GlyphRun(run) => Some(("glyphs", run.anti_aliasing)),
            _ => None,
        })
        .collect();

    assert_eq!(
        modes,
        [
            ("fill", EdgeMode::Enabled),
            ("fill", EdgeMode::Disabled),
            ("glyphs", EdgeMode::Enabled),
            ("glyphs", EdgeMode::Disabled),
        ]
    );
}
