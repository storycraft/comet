use anyrender::{Paint, Scene, recording::RenderCommand};
use comet_layout::{
    fragment::{FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        LayoutFlags, LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{
            AutoValue, Point as LayoutPoint, Rect as LayoutRect, Size, StyleUnit,
            UnitResolveContext,
        },
        taffy::Overflow,
    },
};
use comet_paint::{
    layer::{PaintLayerKey, builder::PaintLayerBuilder, tree::PaintLayerTree},
    painter::CometPainter,
    pre_paint::{
        FragmentData, LayoutInputData, resolver::PrePaintTreeResolver, tree::PrePaintTree,
    },
    style::{BorderRadius, Fill, TextColor, Transform, TransformOrigin},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use kurbo::{Affine, Point, Rect, RoundedRectRadii, Shape};
use peniko::Color;

const VIEWPORT: Size<f64> = Size::new(120.0, 400.0);
const SCENE_VIEWPORT: Rect = Rect::new(0.0, 0.0, 120.0, 400.0);
const UNITS: UnitResolveContext = LayoutConstraint::new_root(VIEWPORT, 16.0).units;

const RED: Color = Color::from_rgb8(200, 20, 20);
const GREEN: Color = Color::from_rgb8(20, 160, 20);
const BLUE: Color = Color::from_rgb8(20, 40, 180);

/// Input, fragment and pre-paint trees of one UI root.
#[derive(Default)]
struct Fixture {
    inputs: LayoutInputTree,
    fragments: FragmentTree,
    pre_paint: PrePaintTree,
    resolver: PrePaintTreeResolver,
    input_builder: LayoutInputTreeBuilder,
    builder: FragmentTreeBuilder,
    root: Option<FragmentKey>,
}

impl Fixture {
    /// Build the layout and resolve the whole pre-paint tree.
    fn build(&mut self, ui: &Ui, node: NodeKey) -> FragmentKey {
        let root = self.layout(ui, node);
        self.resolve(ui, root).unwrap();

        root
    }

    /// Build or update the layout only, keeping the resolved pre-paint tree.
    fn layout(&mut self, ui: &Ui, node: NodeKey) -> FragmentKey {
        let input_root = self
            .input_builder
            .build(
                ui,
                &mut self.inputs,
                BuildRequest::new(node, 16.0, VIEWPORT),
            )
            .unwrap();
        let root = self
            .builder
            .build(
                ui,
                &self.inputs,
                input_root,
                &mut self.fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0),
            )
            .unwrap();
        self.inputs.clear_flags(input_root);
        self.root = Some(root);

        root
    }

    fn resolve(&mut self, ui: &Ui, key: FragmentKey) -> Option<()> {
        self.resolver.resolve(
            ui,
            &self.inputs,
            &self.fragments,
            &UNITS,
            &mut self.pre_paint,
            key,
        )
    }

    fn item(&self, node: NodeKey) -> LayoutInputKey {
        self.inputs.item_for_node(node).unwrap()
    }

    fn box_of(&self, node: NodeKey) -> FragmentKey {
        self.fragments.box_fragment(self.item(node)).unwrap()
    }

    fn data(&self, node: NodeKey) -> &LayoutInputData {
        self.pre_paint.get(self.item(node)).unwrap()
    }

    fn border_radius(&self, key: FragmentKey) -> Option<RoundedRectRadii> {
        self.pre_paint.fragment(key).unwrap().border_radius
    }

    fn fragment(&self, key: FragmentKey) -> &FragmentData {
        self.pre_paint.fragment(key).unwrap()
    }

    /// The data of a node's box fragment.
    fn boxed(&self, node: NodeKey) -> &FragmentData {
        self.fragment(self.box_of(node))
    }

    fn clip(&self, node: NodeKey) -> (bool, bool) {
        self.data(node).clip
    }

    /// The transform of a fragment relative to its parent.
    fn transform(&self, key: FragmentKey) -> Affine {
        self.fragment(key).transform
    }

    fn text_color(&self, node: NodeKey) -> Color {
        solid(&self.data(node).text_color)
    }

    /// Border boxes of the fragments an item links together, each in its own
    /// local coordinates.
    fn linked_rects(&self, node: NodeKey) -> Vec<Rect> {
        self.fragments
            .fragments(self.item(node))
            .map(|key| {
                let size = self.fragments.items[key].layout.size;

                Rect::from_origin_size(Point::ORIGIN, (size.width, size.height))
            })
            .collect()
    }

    /// Build the paint layers of the whole fragment tree, with their root.
    fn layers(&self, ui: &Ui) -> (PaintLayerTree, PaintLayerKey) {
        let mut layers = PaintLayerTree::new();
        let root = PaintLayerBuilder::new()
            .build(
                ui,
                &self.inputs,
                &self.fragments,
                &mut layers,
                self.root.unwrap(),
            )
            .unwrap();

        (layers, root)
    }

    fn draw(&self, ui: &Ui) -> Scene {
        let mut scene = Scene::new();
        let (layers, root) = self.layers(ui);
        CometPainter::new().draw(
            ui,
            &self.inputs,
            &self.fragments,
            &self.pre_paint,
            &layers,
            root,
            SCENE_VIEWPORT,
            &mut scene,
        );

        scene
    }
}

fn solid(paint: &Paint) -> Color {
    match paint {
        Paint::Solid(color) => *color,
        _ => panic!("test only uses solid paints"),
    }
}

fn px(value: f64) -> AutoValue<StyleUnit> {
    AutoValue::Value(StyleUnit::Px(value))
}

fn fixed(width: f64, height: f64) -> bx::Size {
    bx::Size(Size::new(px(width), px(height)))
}

fn text(ui: &mut Ui, parent: NodeKey, value: &str) -> NodeKey {
    let node = ui.create(Node::Text, (Text(value.into()),));
    ui.append(parent, node);
    node
}

fn hidden() -> bx::Overflow {
    bx::Overflow(LayoutPoint::new(Overflow::Hidden, Overflow::Hidden))
}

fn assert_affine(actual: Affine, expected: Affine) {
    let (actual, expected) = (actual.as_coeffs(), expected.as_coeffs());

    for (a, b) in actual.iter().zip(expected.iter()) {
        assert!((a - b).abs() < 1e-9, "{actual:?} != {expected:?}");
    }
}

/// Fills, clip layers and pops of a scene, with their bounding boxes.
fn commands(scene: &Scene) -> Vec<String> {
    scene
        .commands
        .iter()
        .map(|command| match command {
            RenderCommand::Fill(fill) => format!(
                "fill {:?}",
                fill.transform
                    .transform_rect_bbox(fill.shape.bounding_box())
            ),
            RenderCommand::PushClipLayer(clip) => format!(
                "clip {:?}",
                clip.transform.transform_rect_bbox(clip.clip.bounding_box())
            ),
            RenderCommand::PopLayer => "pop".into(),
            _ => "other".into(),
        })
        .collect()
}

#[test]
fn fragments_resolve_transforms_relative_to_their_parents() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (bx::Padding(LayoutRect::new_all(StyleUnit::Px(5.0))),),
    );
    let block = ui.create(Node::Box, (fixed(40.0, 20.0),));
    ui.append(root, block);
    let content = text(&mut ui, block, "hi");

    let mut f = Fixture::default();
    let root_fragment = f.build(&ui, root);

    assert_eq!(f.transform(root_fragment), Affine::IDENTITY);
    assert_eq!(f.transform(f.box_of(block)), Affine::translate((5.0, 5.0)));

    // Boxes stitch their own fragment: lines owned by the block do not count.
    let block_fragment = f.fragment(f.box_of(block));
    assert_eq!(block_fragment.transform_origin, Point::new(20.0, 10.0));
    assert_eq!(
        block_fragment.inverse_transform,
        Some(Affine::translate((-5.0, -5.0)))
    );
    assert_eq!(f.clip(block), (false, false));
    assert_eq!(f.data(block).stitched_size, kurbo::Size::new(40.0, 20.0));
    assert_eq!(f.data(root).stitched_size, kurbo::Size::new(120.0, 30.0));

    // Each glyph run has its own origin, at its centre.
    let runs: Vec<_> = f.fragments.fragments(f.item(content)).collect();
    assert!(!runs.is_empty());
    for (key, rect) in runs.into_iter().zip(f.linked_rects(content)) {
        assert_eq!(f.fragment(key).transform_origin, rect.center());
    }
}

#[test]
fn wrapped_inline_elements_stitch_their_fragments_across_lines() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    ui.append(root, span);
    text(
        &mut ui,
        span,
        "one two three four five six seven eight nine ten",
    );

    let mut f = Fixture::default();
    f.build(&ui, root);

    let rects = f.linked_rects(span);
    assert!(rects.len() > 1);

    // Stitched side by side, as if on one line.
    let stitched = kurbo::Size::new(
        rects.iter().map(Rect::width).sum(),
        rects.iter().map(Rect::height).fold(0.0, f64::max),
    );

    assert_eq!(f.data(span).stitched_size, stitched);

    // Each fragment is transformed around its own centre.
    let fragments: Vec<_> = f.fragments.fragments(f.item(span)).collect();
    for (key, rect) in fragments.into_iter().zip(rects) {
        assert_eq!(f.fragment(key).transform_origin, rect.center());
    }
}

#[test]
fn wrapped_fragments_resolve_percentage_radii_against_their_stitched_size() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            BorderRadius(LayoutRect::new_all(StyleUnit::Percent(0.5))),
        ),
    );
    ui.append(root, span);
    text(
        &mut ui,
        span,
        "one two three four five six seven eight nine ten",
    );

    let mut f = Fixture::default();
    f.build(&ui, root);

    let fragments: Vec<_> = f.fragments.fragments(f.item(span)).collect();
    assert!(fragments.len() > 1);

    // The stitched span is wider than tall, so the radius is half its height,
    // whatever the width of each fragment.
    let stitched = f.data(span).stitched_size;
    assert!(stitched.width > stitched.height);
    let radius = stitched.height / 2.0;

    let first = f.border_radius(fragments[0]).unwrap();
    assert_eq!(first, RoundedRectRadii::new(radius, 0.0, 0.0, radius));
}

#[test]
fn fragment_boxes_resolve_their_border_radii() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let rounded = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            BorderRadius(LayoutRect::new_invidiual(
                StyleUnit::Px(3.0),
                StyleUnit::Em(0.25),
                StyleUnit::Rem(0.5),
                StyleUnit::Percent(0.5),
            )),
        ),
    );
    ui.append(root, rounded);
    let span = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            BorderRadius(LayoutRect::new_all(StyleUnit::Px(3.0))),
        ),
    );
    ui.append(root, span);
    text(
        &mut ui,
        span,
        "one two three four five six seven eight nine ten",
    );

    let mut f = Fixture::default();
    let root_fragment = f.build(&ui, root);

    // Items without a radius, and lines, have none.
    assert_eq!(f.border_radius(root_fragment), None);
    for line in f.fragments.items.children(root_fragment) {
        if f.fragments.items[line].item.is_box() {
            continue;
        }
        assert_eq!(f.border_radius(line), None);
    }

    // Percentages resolve against the shorter side, units with the item's
    // font size.
    assert_eq!(
        f.border_radius(f.box_of(rounded)),
        Some(RoundedRectRadii::new(3.0, 4.0, 8.0, 10.0))
    );

    // Only the outer corners of wrapped fragments are rounded.
    let fragments: Vec<_> = f.fragments.fragments(f.item(span)).collect();
    assert!(fragments.len() > 1);

    for (index, &fragment) in fragments.iter().enumerate() {
        let left = if index == 0 { 3.0 } else { 0.0 };
        let right = if index == fragments.len() - 1 {
            3.0
        } else {
            0.0
        };

        assert_eq!(
            f.border_radius(fragment),
            Some(RoundedRectRadii::new(left, right, right, left))
        );
    }
}

#[test]
fn text_color_is_inherited_through_the_ui() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (TextColor(Paint::Solid(BLUE)),));
    let plain = text(&mut ui, root, "blue ");
    let span = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, TextColor(Paint::Solid(RED))),
    );
    ui.append(root, span);
    let before = text(&mut ui, span, "red ");
    // A block inside the span is a fragment child of the root box, but still
    // inherits from the span.
    let block = ui.create(Node::Box, (fixed(10.0, 10.0),));
    ui.append(span, block);
    let inside = text(&mut ui, block, "still red");
    let green = text(&mut ui, span, "green");
    ui.set_props(green, (TextColor(Paint::Solid(GREEN)),));

    let mut f = Fixture::default();
    let root_fragment = f.build(&ui, root);

    assert_eq!(
        f.fragments.items.parent(f.box_of(block)),
        Some(root_fragment)
    );
    assert_eq!(f.text_color(plain), BLUE);
    assert_eq!(f.text_color(before), RED);
    assert_eq!(f.text_color(block), RED);
    assert_eq!(f.text_color(inside), RED);
    assert_eq!(f.text_color(green), GREEN);
}

#[test]
fn transforms_multiply_around_their_origins() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(
        Node::Box,
        (fixed(100.0, 40.0), Transform(Affine::scale(2.0))),
    );
    let inner = ui.create(
        Node::Box,
        (
            fixed(20.0, 10.0),
            Transform(Affine::translate((5.0, 0.0))),
            Fill(Paint::Solid(RED)),
        ),
    );
    let corner = ui.create(
        Node::Box,
        (
            fixed(20.0, 10.0),
            Transform(Affine::scale(3.0)),
            TransformOrigin(LayoutPoint::new(StyleUnit::Px(0.0), StyleUnit::Px(0.0))),
        ),
    );
    ui.append(root, outer);
    ui.append(outer, inner);
    ui.append(outer, corner);

    let mut f = Fixture::default();
    f.build(&ui, root);

    let outer_transform =
        Affine::translate((50.0, 20.0)) * Affine::scale(2.0) * Affine::translate((-50.0, -20.0));

    assert_eq!(f.boxed(outer).transform_origin, Point::new(50.0, 20.0));
    assert_affine(f.boxed(outer).transform, outer_transform);
    // A translation is the same around any origin. Ancestor transforms are
    // not included.
    assert_affine(f.boxed(inner).transform, Affine::translate((5.0, 0.0)));

    // Placed at its location in the outer box, then scaled around its corner.
    assert_eq!(f.boxed(corner).transform_origin, Point::ORIGIN);
    assert_affine(
        f.boxed(corner).transform,
        Affine::translate((0.0, 10.0)) * Affine::scale(3.0),
    );

    // The painter draws in local coordinates, multiplying transforms down the
    // tree.
    let scene = f.draw(&ui);
    let fill = scene
        .commands
        .iter()
        .find_map(|command| match command {
            RenderCommand::Fill(fill) => Some(fill),
            _ => None,
        })
        .unwrap();

    assert_affine(
        fill.transform,
        outer_transform * Affine::translate((5.0, 0.0)),
    );
    assert_eq!(fill.shape.bounding_box(), Rect::new(0.0, 0.0, 20.0, 10.0));
}

#[test]
fn overflow_clips_contents_to_the_fragment_box() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (bx::Padding(LayoutRect::new_all(StyleUnit::Px(5.0))),),
    );
    let clipper = ui.create(
        Node::Box,
        (
            fixed(50.0, 30.0),
            bx::Border(LayoutRect::new_all(StyleUnit::Px(2.0))),
            hidden(),
            Fill(Paint::Solid(RED)),
        ),
    );
    let colored = ui.create(
        Node::Box,
        (
            fixed(10.0, 4.0),
            TextColor(Paint::Solid(GREEN)),
            Fill(Paint::Solid(GREEN)),
        ),
    );
    // Clips horizontally only, inside the clipper.
    let column = ui.create(
        Node::Box,
        (
            bx::Size(Size::new(px(20.0), AutoValue::Auto)),
            bx::Overflow(LayoutPoint::new(Overflow::Clip, Overflow::Visible)),
        ),
    );
    ui.append(root, clipper);
    ui.append(clipper, colored);
    ui.append(clipper, column);
    let tall = ui.create(Node::Box, (fixed(40.0, 100.0), Fill(Paint::Solid(BLUE))));
    ui.append(column, tall);

    let mut f = Fixture::default();
    f.build(&ui, root);

    assert_eq!(f.clip(clipper), (true, true));
    assert_eq!(f.clip(colored), (false, false));
    assert_eq!(f.clip(column), (true, false));

    // A box is painted outside its own clip; its contents inside it. Clip
    // layers nest, and unclipped axes are bounded by the viewport.
    assert_eq!(
        commands(&f.draw(&ui)),
        [
            format!("fill {:?}", Rect::new(5.0, 5.0, 55.0, 35.0)),
            format!("clip {:?}", Rect::new(5.0, 5.0, 55.0, 35.0)),
            format!("fill {:?}", Rect::new(7.0, 7.0, 17.0, 11.0)),
            format!("clip {:?}", Rect::new(7.0, 0.0, 27.0, 400.0)),
            format!("fill {:?}", Rect::new(7.0, 11.0, 47.0, 111.0)),
            "pop".into(),
            "pop".into(),
        ]
    );
}

#[test]
fn only_box_fragments_clip() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline, hidden()));
    ui.append(root, span);
    text(&mut ui, span, "inline");

    let mut f = Fixture::default();
    f.build(&ui, root);

    // The item clips, but its inline fragments push no clip layer.
    assert_eq!(f.clip(span), (true, true));
    assert!(
        !commands(&f.draw(&ui))
            .iter()
            .any(|command| command.starts_with("clip"))
    );
}

#[test]
fn clips_follow_the_transform_of_their_item() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let clipper = ui.create(
        Node::Box,
        (fixed(40.0, 20.0), hidden(), Transform(Affine::rotate(0.3))),
    );
    let content = ui.create(Node::Box, (fixed(40.0, 20.0), Fill(Paint::Solid(RED))));
    ui.append(root, clipper);
    ui.append(clipper, content);

    let mut f = Fixture::default();
    f.build(&ui, root);

    let scene = f.draw(&ui);
    let clip = scene
        .commands
        .iter()
        .find_map(|command| match command {
            RenderCommand::PushClipLayer(clip) => Some(clip),
            _ => None,
        })
        .unwrap();

    // The bounding box is clipped exactly, under the item's transform.
    assert_affine(clip.transform, f.boxed(clipper).transform);
    assert_eq!(clip.clip.bounding_box(), Rect::new(0.0, 0.0, 40.0, 20.0));
}

#[test]
fn clips_round_with_the_border_radius_on_both_axes() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let radius = || BorderRadius(LayoutRect::new_all(StyleUnit::Px(5.0)));
    let rounded = ui.create(Node::Box, (fixed(40.0, 20.0), hidden(), radius()));
    // Clips horizontally only: its corners lie on the viewport bound.
    let column = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            bx::Overflow(LayoutPoint::new(Overflow::Clip, Overflow::Visible)),
            radius(),
        ),
    );
    for parent in [rounded, column] {
        let content = ui.create(Node::Box, (fixed(40.0, 20.0), Fill(Paint::Solid(RED))));
        ui.append(root, parent);
        ui.append(parent, content);
    }

    let mut f = Fixture::default();
    f.build(&ui, root);

    let scene = f.draw(&ui);
    let areas: Vec<_> = scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::PushClipLayer(clip) => Some(clip.clip.area().abs()),
            _ => None,
        })
        .collect();

    let rounded_area = Rect::new(0.0, 0.0, 40.0, 20.0).to_rounded_rect(5.0).area();
    assert_eq!(areas.len(), 2);
    assert!((areas[0] - rounded_area).abs() < 0.1, "{areas:?}");
    assert_eq!(areas[1], 40.0 * 400.0);
}

#[test]
fn content_outside_its_clip_is_not_painted() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let clipper = ui.create(Node::Box, (fixed(50.0, 20.0), hidden()));
    let inside = ui.create(Node::Box, (fixed(50.0, 20.0), Fill(Paint::Solid(RED))));
    // Boxes touching the clip edge are painted, so leave a gap.
    let gap = ui.create(Node::Box, (fixed(50.0, 5.0),));
    let outside = ui.create(Node::Box, (fixed(50.0, 20.0), Fill(Paint::Solid(BLUE))));
    ui.append(root, clipper);
    ui.append(clipper, inside);
    ui.append(clipper, gap);
    ui.append(clipper, outside);

    let mut f = Fixture::default();
    f.build(&ui, root);
    let fills: Vec<_> = f
        .draw(&ui)
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::Fill(fill) => Some(solid(&fill.brush)),
            _ => None,
        })
        .collect();

    assert_eq!(fills, [RED]);
}

#[test]
fn resolving_a_subtree_follows_the_layout() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let top = ui.create(Node::Box, (fixed(10.0, 10.0),));
    let clipper = ui.create(
        Node::Box,
        (
            fixed(50.0, 20.0),
            hidden(),
            Transform(Affine::translate((3.0, 0.0))),
        ),
    );
    ui.append(root, top);
    ui.append(root, clipper);

    let mut f = Fixture::default();
    f.build(&ui, root);
    let clipper_fragment = f.box_of(clipper);
    let root_data = f.boxed(root).clone();

    ui.set_props(top, (fixed(10.0, 30.0),));
    f.inputs.mark_node(top, LayoutFlags::BOX_DIRTY);
    f.layout(&ui, root);

    // Resolved data is kept until it is resolved again.
    assert_affine(
        f.transform(clipper_fragment),
        Affine::translate((3.0, 10.0)),
    );

    // Resolving the subtree leaves its ancestors as they are.
    f.resolve(&ui, clipper_fragment).unwrap();

    assert_eq!(f.boxed(clipper).transform_origin, Point::new(25.0, 10.0));
    assert_affine(
        f.transform(clipper_fragment),
        Affine::translate((3.0, 30.0)),
    );
    assert_eq!(f.boxed(root), &root_data);
}

#[test]
fn resolving_a_subtree_looks_up_its_paint_styles_again() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (TextColor(Paint::Solid(BLUE)),));
    let colored = ui.create(Node::Box, (TextColor(Paint::Solid(RED)),));
    let block = ui.create(Node::Box, ());
    ui.append(root, colored);
    ui.append(colored, block);
    let content = text(&mut ui, block, "text");

    let mut f = Fixture::default();
    f.build(&ui, root);

    // Paint style changes are only looked up when resolving.
    ui.set_props(colored, (TextColor(Paint::Solid(GREEN)),));
    assert_eq!(f.text_color(colored), RED);

    f.resolve(&ui, f.box_of(colored)).unwrap();

    assert_eq!(f.text_color(root), BLUE);
    assert_eq!(f.text_color(colored), GREEN);
    assert_eq!(f.text_color(block), GREEN);
    assert_eq!(f.text_color(content), GREEN);
}

#[test]
fn resolving_a_fragment_resolves_only_its_unresolved_ancestor_items() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (TextColor(Paint::Solid(RED)),));
    let block = ui.create(Node::Box, ());
    ui.append(root, block);
    let content = text(&mut ui, block, "text");

    let mut f = Fixture::default();
    let root_fragment = f.layout(&ui, root);
    f.resolve(&ui, f.box_of(block)).unwrap();

    // Fragments do not depend on their ancestors, items inherit from theirs.
    assert!(f.pre_paint.fragment(root_fragment).is_none());
    assert_eq!(f.transform(f.box_of(block)), Affine::IDENTITY);
    assert_eq!(f.text_color(root), RED);
    assert_eq!(f.text_color(content), RED);
}

#[test]
fn resolving_fails_for_nodes_deleted_since_the_layout_was_built() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let block = ui.create(Node::Box, ());
    ui.append(root, block);
    text(&mut ui, block, "text");

    let mut f = Fixture::default();
    let root_fragment = f.build(&ui, root);

    // The layout still holds the deleted node until it is rebuilt.
    ui.delete(block);
    assert_eq!(f.resolve(&ui, root_fragment), None);

    // Resolving succeeds again once the layout follows the UI.
    f.inputs.mark_node(root, LayoutFlags::FULL_DIRTY);
    let root_fragment = f.layout(&ui, root);
    assert_eq!(f.resolve(&ui, root_fragment), Some(()));
}

#[test]
fn inverse_transforms_map_parent_coordinates_back() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(
        Node::Box,
        (
            fixed(100.0, 40.0),
            Transform(Affine::rotate(0.4) * Affine::scale(2.0)),
        ),
    );
    let inner = ui.create(
        Node::Box,
        (fixed(20.0, 10.0), Transform(Affine::translate((5.0, 3.0)))),
    );
    ui.append(root, outer);
    ui.append(outer, inner);

    let mut f = Fixture::default();
    f.build(&ui, root);

    assert_eq!(f.boxed(root).inverse_transform, Some(Affine::IDENTITY));

    for node in [outer, inner] {
        let data = f.boxed(node);
        assert_affine(
            data.inverse_transform.unwrap() * data.transform,
            Affine::IDENTITY,
        );
    }
}

#[test]
fn collapsed_fragments_have_no_inverse_transform() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    // Scaled to zero width, and clipping: it collapses with its contents.
    let collapsed = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            hidden(),
            Transform(Affine::scale_non_uniform(0.0, 1.0)),
        ),
    );
    let content = ui.create(Node::Box, (fixed(40.0, 20.0), Fill(Paint::Solid(RED))));
    ui.append(root, collapsed);
    ui.append(collapsed, content);

    let mut f = Fixture::default();
    f.build(&ui, root);

    // Its contents keep their own inverse, relative to the collapsed box.
    assert_eq!(f.boxed(collapsed).inverse_transform, None);
    assert_eq!(f.boxed(content).inverse_transform, Some(Affine::IDENTITY));

    // No clip layer is pushed for a box that covers no area.
    assert!(
        !commands(&f.draw(&ui))
            .iter()
            .any(|command| command.starts_with("clip"))
    );
}
