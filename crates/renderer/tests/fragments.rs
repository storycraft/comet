use anyrender::{Glyph, Paint, Scene, recording::RenderCommand};
use comet_layout::{
    fragment::{FragmentItem, FragmentKey, builder::FragmentBuilder, tree::FragmentTree},
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Rect as LayoutRect, Size, StyleUnit},
    },
    tree::{LayoutTree, builder::LayoutTreeBuilder},
};
use comet_renderer::{
    CometRenderer,
    style::{BorderFill, Fill, TextColor},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use kurbo::{Rect, Shape};
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

fn build(ui: &Ui, node: NodeKey) -> (FragmentTree, FragmentKey) {
    let mut layout = LayoutTree::new();
    let root = LayoutTreeBuilder::new()
        .build(ui, node, &mut layout, None)
        .unwrap();

    let mut fragments = FragmentTree::new();
    let root = FragmentBuilder::new()
        .build(
            &LayoutConstraint::new_root(Size::new(120.0, 400.0), 16.0),
            ui,
            &layout,
            root,
            &mut fragments,
        )
        .unwrap();

    (fragments, root)
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

    let (mut tree, root) = build(&ui, root_node);
    let mut renderer = CometRenderer::new();
    let mut scene = Scene::new();

    renderer.draw(&ui, &tree, root, &mut scene);

    let mut pending = vec![(root, 0.0, 0.0)];
    let mut expected_fills = Vec::new();
    let mut expected_glyphs = 0;

    while let Some((key, parent_x, parent_y)) = pending.pop() {
        let fragment = &tree.items[key];
        let x = parent_x + fragment.layout.location.x;
        let y = parent_y + fragment.layout.location.y;

        if fragment.node == Some(span) {
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
            for item in tree.inline_layouts[&fragment.source]
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

    renderer.draw(&ui, &tree, root, &mut shifted);

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
            bx::Border(LayoutRect::new_axis(StyleUnit::Px(2.0), StyleUnit::Px(2.0))),
            BorderFill(Paint::Solid(Color::BLACK)),
            Fill(Paint::Solid(Color::WHITE)),
            TextColor(Paint::Solid(red)),
        ),
    );
    let text = ui.create(Node::Text, (Text("atom".into()),));

    ui.append(root_node, before);
    ui.append(root_node, atomic);
    ui.append(atomic, text);

    let (tree, root) = build(&ui, root_node);
    let mut scene = Scene::new();

    CometRenderer::new().draw(&ui, &tree, root, &mut scene);

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

    let (tree, root) = build(&ui, node);
    let mut scene = Scene::new();

    CometRenderer::new().draw(&ui, &tree, root, &mut scene);

    assert_eq!(fills(&scene).len(), 1);
    assert!(!glyphs(&scene).is_empty());
}
