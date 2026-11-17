use anyrender::{Paint, Scene, recording::RenderCommand};
use comet_layout::{
    fragment::{FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        InputFlags, LayoutInputKey,
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
    style::{
        BorderRadius, Fill, OutlineFill, OutlineOffset, OutlineWidth, TextColor, Transform,
        TransformOrigin,
    },
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
    input_root: Option<LayoutInputKey>,
}

impl Fixture {
    /// Build the layout, resolve the pre-paint tree and clear the flags.
    fn build(&mut self, ui: &Ui, node: NodeKey) -> FragmentKey {
        let root = self.layout(ui, node);
        self.resolve(ui, root).unwrap();
        self.clear_flags();

        root
    }

    /// Clear the input flags, once the pre-paint tree was resolved.
    fn clear_flags(&mut self) {
        if let Some(input_root) = self.input_root {
            self.inputs.clear_flags(input_root);
        }
    }

    /// Build or update the layout only, keeping the resolved pre-paint tree
    /// and the input flags for the next resolve.
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
        self.input_root = Some(input_root);
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

    /// Mark the item of a node as needing pre-paint.
    fn mark(&mut self, node: NodeKey) {
        self.inputs.mark_node(node, InputFlags::NEED_PRE_PAINT);
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
        self.pre_paint.fragment(key)?.border_radius
    }

    /// The border radii of a fragment, square at its joins, or `None` if it
    /// has no border radius.
    fn border_radii(&self, key: FragmentKey) -> Option<RoundedRectRadii> {
        let data = self.pre_paint.fragment(key)?;
        data.border_radius?;

        Some(data.border_radii(&self.fragments.items[key]))
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

    /// Colors of the glyph runs painted, in paint order, with consecutive
    /// equal colors merged.
    fn text_colors(&self, ui: &Ui) -> Vec<Color> {
        let mut colors: Vec<Color> = self
            .draw(ui)
            .commands
            .iter()
            .filter_map(|command| match command {
                RenderCommand::GlyphRun(run) => Some(solid(&run.brush)),
                _ => None,
            })
            .collect();
        colors.dedup();

        colors
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
    fn layers(&self) -> (PaintLayerTree, PaintLayerKey) {
        let mut layers = PaintLayerTree::new();
        let root = PaintLayerBuilder::new()
            .build(
                &self.fragments,
                &self.inputs,
                &self.pre_paint,
                &mut layers,
                self.input_root.unwrap(),
            )
            .unwrap();

        (layers, root)
    }

    fn draw(&self, ui: &Ui) -> Scene {
        let mut scene = Scene::new();
        let (layers, root) = self.layers();
        CometPainter::new().draw(
            ui,
            &self.inputs,
            &self.fragments,
            &self.pre_paint,
            &layers,
            root,
            SCENE_VIEWPORT,
            1.0,
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

    // The block is transformed around the centre of its own border box.
    let block_fragment = f.fragment(f.box_of(block));
    assert_eq!(block_fragment.transform_origin, Point::new(20.0, 10.0));
    assert_eq!(
        block_fragment.inverse_transform,
        Some(Affine::translate((-5.0, -5.0)))
    );
    assert_eq!(f.clip(block), (false, false));

    // Each glyph run has its own origin, at its centre.
    let runs: Vec<_> = f.fragments.fragments(f.item(content)).collect();
    assert!(!runs.is_empty());
    for (key, rect) in runs.into_iter().zip(f.linked_rects(content)) {
        assert_eq!(f.fragment(key).transform_origin, rect.center());
    }
}

#[test]
fn wrapped_inline_elements_transform_each_fragment_around_its_centre() {
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

    // Each fragment is transformed around its own centre.
    let fragments: Vec<_> = f.fragments.fragments(f.item(span)).collect();
    for (key, rect) in fragments.into_iter().zip(rects) {
        assert_eq!(f.fragment(key).transform_origin, rect.center());
    }
}

#[test]
fn split_fragments_square_joins_and_resolve_percentages_against_their_own_size() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            BorderRadius(LayoutRect::new_all(StyleUnit::Px(6.0))),
        ),
    );
    let relative = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            BorderRadius(LayoutRect::new_all(StyleUnit::Percent(0.5))),
        ),
    );
    let split = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            BorderRadius(LayoutRect::new_all(StyleUnit::Percent(0.5))),
        ),
    );
    ui.append(root, span);
    ui.append(root, relative);
    ui.append(root, split);
    text(&mut ui, split, "one two three four five six seven");
    text(
        &mut ui,
        span,
        "one two three four five six seven eight nine ten",
    );

    let mut f = Fixture::default();
    f.build(&ui, root);

    let fragments: Vec<_> = f.fragments.fragments(f.item(span)).collect();
    assert!(fragments.len() > 1);

    // Every fragment resolves all four corners, and its border squares the
    // corners at its joins.
    for &fragment in &fragments {
        assert_eq!(f.border_radius(fragment), Some(RoundedRectRadii::from(6.0)));
    }
    assert_eq!(
        f.border_radii(fragments[0]),
        Some(RoundedRectRadii::new(6.0, 0.0, 0.0, 6.0))
    );
    assert_eq!(
        f.border_radii(*fragments.last().unwrap()),
        Some(RoundedRectRadii::new(0.0, 6.0, 6.0, 0.0))
    );
    for &fragment in &fragments[1..fragments.len() - 1] {
        assert_eq!(f.border_radii(fragment), Some(RoundedRectRadii::from(0.0)));
    }

    // Hit testing follows the border: the joined corner is square, the outer
    // one rounded.
    let first = fragments[0];
    let width = f.fragments.items[first].layout.size.width;
    assert!(
        f.pre_paint
            .hit_local(&f.fragments, first, Point::new(width - 0.5, 0.5))
    );
    assert!(
        !f.pre_paint
            .hit_local(&f.fragments, first, Point::new(0.5, 0.5))
    );

    // Percentages resolve against the smaller side of each fragment.
    assert_eq!(
        f.border_radius(f.box_of(relative)),
        Some(RoundedRectRadii::from(10.0))
    );

    let split_fragments: Vec<_> = f.fragments.fragments(f.item(split)).collect();
    assert!(split_fragments.len() > 1);
    for (index, &fragment) in split_fragments.iter().enumerate() {
        let size = f.fragments.items[fragment].layout.size;
        let radius = size.width.min(size.height) * 0.5;
        assert!(radius > 0.0);
        let left = if index == 0 { radius } else { 0.0 };
        let right = if index == split_fragments.len() - 1 {
            radius
        } else {
            0.0
        };

        assert_eq!(
            f.border_radius(fragment),
            Some(RoundedRectRadii::from(radius))
        );
        assert_eq!(
            f.border_radii(fragment),
            Some(RoundedRectRadii::new(left, right, right, left))
        );
    }
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

    // Units resolve with the item's font size, percentages against the
    // smaller side.
    assert_eq!(
        f.border_radius(f.box_of(rounded)),
        Some(RoundedRectRadii::new(3.0, 4.0, 8.0, 10.0))
    );

    // Only the outer corners of wrapped fragments' borders are rounded.
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
            f.border_radii(fragment),
            Some(RoundedRectRadii::new(left, right, right, left))
        );
    }
}

#[test]
fn text_color_is_inherited_through_the_ui() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (TextColor(Paint::Solid(BLUE)),));
    text(&mut ui, root, "blue ");
    let span = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, TextColor(Paint::Solid(RED))),
    );
    ui.append(root, span);
    text(&mut ui, span, "red ");
    // A block inside the span is a fragment child of the root box, but still
    // inherits from the span.
    let block = ui.create(Node::Box, (fixed(10.0, 10.0),));
    ui.append(span, block);
    text(&mut ui, block, "still red");
    let green = text(&mut ui, span, "green");
    ui.set_props(green, (TextColor(Paint::Solid(GREEN)),));

    let mut f = Fixture::default();
    let root_fragment = f.build(&ui, root);

    assert_eq!(
        f.fragments.items.parent(f.box_of(block)),
        Some(root_fragment)
    );
    // Blue, then red before and inside the block, then green.
    assert_eq!(f.text_colors(&ui), [BLUE, RED, GREEN]);
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
fn resolving_follows_the_layout() {
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
    f.inputs.mark_node(top, InputFlags::NEED_BOX_LAYOUT);
    f.layout(&ui, root);

    // Resolved data is kept until it is resolved again.
    assert_affine(
        f.transform(clipper_fragment),
        Affine::translate((3.0, 10.0)),
    );

    // Resolving follows the moved and resized fragments.
    let root_fragment = f.root.unwrap();
    f.resolve(&ui, root_fragment).unwrap();

    assert_eq!(f.boxed(clipper).transform_origin, Point::new(25.0, 10.0));
    assert_affine(
        f.transform(clipper_fragment),
        Affine::translate((3.0, 30.0)),
    );
    // The root grew, so its origin moved, but it stays in place.
    assert_eq!(f.boxed(root).transform, root_data.transform);
    assert_ne!(f.boxed(root).transform_origin, root_data.transform_origin);
    f.assert_matches_full_resolve(&ui, root_fragment);
}

#[test]
fn text_color_changes_paint_without_resolving() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (TextColor(Paint::Solid(BLUE)),));
    text(&mut ui, root, "blue");
    let colored = ui.create(Node::Box, (TextColor(Paint::Solid(RED)),));
    let block = ui.create(Node::Box, ());
    ui.append(root, colored);
    ui.append(colored, block);
    text(&mut ui, block, "text");

    let mut f = Fixture::default();
    f.build(&ui, root);

    assert_eq!(f.text_colors(&ui), [BLUE, RED]);

    // Text colors are looked up while painting, inherited by the nested
    // text, so the change shows without resolving the pre-paint tree.
    ui.set_props(colored, (TextColor(Paint::Solid(GREEN)),));
    assert_eq!(f.text_colors(&ui), [BLUE, GREEN]);

    // Without a color of its own, the box inherits again.
    ui.remove_prop::<TextColor>(colored);
    assert_eq!(f.text_colors(&ui), [BLUE]);
}

#[test]
fn resolving_a_fragment_leaves_its_ancestors_unresolved() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (TextColor(Paint::Solid(RED)),));
    let block = ui.create(Node::Box, ());
    ui.append(root, block);
    let content = text(&mut ui, block, "text");

    let mut f = Fixture::default();
    let root_fragment = f.layout(&ui, root);
    f.resolve(&ui, f.box_of(block)).unwrap();

    // Neither fragments nor items depend on their ancestors.
    assert!(f.pre_paint.fragment(root_fragment).is_none());
    assert!(f.pre_paint.get(f.item(root)).is_none());
    assert_eq!(f.transform(f.box_of(block)), Affine::IDENTITY);
    assert!(f.pre_paint.get(f.item(block)).is_some());
    assert!(f.pre_paint.get(f.item(content)).is_some());
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

    // The layout still holds the deleted node until it is rebuilt, so a
    // fragment of it fails to resolve.
    f.mark(block);
    ui.delete(block);
    assert_eq!(f.resolve(&ui, root_fragment), None);

    // Resolving succeeds again once the layout follows the UI.
    f.inputs.mark_node(root, InputFlags::NEED_CHILDREN);
    let root_fragment = f.layout(&ui, root);
    assert_eq!(f.resolve(&ui, root_fragment), Some(()));
    f.clear_flags();
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

#[test]
fn local_hits_follow_the_rounded_border_box() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let rounded = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            BorderRadius(LayoutRect::new_all(StyleUnit::Px(10.0))),
        ),
    );
    ui.append(root, rounded);
    text(&mut ui, root, "text");

    let mut f = Fixture::default();
    let root_fragment = f.build(&ui, root);
    let rounded = f.box_of(rounded);
    let hit = |key, x, y| f.pre_paint.hit_local(&f.fragments, key, Point::new(x, y));

    // Local coordinates start at the border box corner, whatever its layout
    // location.
    assert!(hit(rounded, 20.0, 10.0));
    assert!(hit(rounded, 0.0, 10.0));
    assert!(!hit(rounded, 1.0, 1.0));
    assert!(!hit(rounded, 41.0, 10.0));
    assert!(!hit(rounded, -1.0, 10.0));

    // Any kind of fragment is hit inside its box, line boxes included.
    let line = f
        .fragments
        .items
        .children(root_fragment)
        .find(|&child| !f.fragments.items[child].item.is_box())
        .unwrap();
    assert!(hit(line, 1.0, 1.0));
    assert!(!hit(line, -1.0, 1.0));

    // Fragments missing from the trees are not hit instead of panicking.
    assert!(!hit(FragmentKey::default(), 1.0, 1.0));
}

#[test]
fn resolving_marked_fragments_leaves_unmarked_descendants_as_they_are() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let parent = ui.create(Node::Box, (fixed(40.0, 20.0),));
    let child = ui.create(Node::Box, (fixed(10.0, 10.0),));
    ui.append(root, parent);
    ui.append(parent, child);

    let mut f = Fixture::default();
    f.build(&ui, root);
    let child_data = f.boxed(child).clone();

    ui.set_props(parent, (hidden(), Transform(Affine::scale(2.0))));
    ui.set_props(child, (hidden(), Transform(Affine::scale(3.0))));
    f.mark(parent);
    f.resolve(&ui, f.root.unwrap()).unwrap();
    f.clear_flags();

    // Only the parent's item and fragment are resolved again.
    assert_eq!(f.clip(parent), (true, true));
    assert_eq!(f.boxed(parent).stacking, Some(0));
    assert_eq!(f.clip(child), (false, false));
    assert_eq!(f.boxed(child), &child_data);

    // Marking the child reaches it.
    f.mark(child);
    f.resolve(&ui, f.root.unwrap()).unwrap();

    assert_eq!(f.clip(child), (true, true));
    assert_eq!(f.boxed(child).stacking, Some(0));
}

#[test]
fn outlines_resolve_without_a_percentage_basis() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let lengths = ui.create(
        Node::Box,
        (
            fixed(10.0, 10.0),
            OutlineWidth(StyleUnit::Em(0.25)),
            OutlineOffset(StyleUnit::Px(-2.0)),
        ),
    );
    let relative = ui.create(
        Node::Box,
        (
            fixed(10.0, 10.0),
            OutlineWidth(StyleUnit::Percent(0.5)),
            OutlineOffset(StyleUnit::Percent(0.5)),
        ),
    );
    let negative = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), OutlineWidth(StyleUnit::Px(-3.0))),
    );
    for node in [lengths, relative, negative] {
        ui.append(root, node);
    }

    let mut f = Fixture::default();
    f.build(&ui, root);

    assert_eq!(f.data(lengths).outline_width, 4.0);
    assert_eq!(f.data(lengths).outline_offset, -2.0);
    assert_eq!(f.data(relative).outline_width, 0.0);
    assert_eq!(f.data(relative).outline_offset, 0.0);
    assert_eq!(f.data(negative).outline_width, 0.0);
    assert_eq!(f.data(root).outline_width, 0.0);
    assert_eq!(f.data(root).outline_offset, 0.0);
}

/// Strokes of a scene, with their widths and path bounding boxes in scene
/// coordinates.
fn strokes(scene: &Scene) -> Vec<(f64, Rect)> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::Stroke(stroke) => Some((
                stroke.style.width,
                stroke
                    .transform
                    .transform_rect_bbox(stroke.shape.bounding_box()),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn outlines_stroke_outside_the_offset_box() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (bx::Padding(LayoutRect::new_all(StyleUnit::Px(10.0))),),
    );
    let outlined = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            OutlineWidth(StyleUnit::Px(2.0)),
            OutlineOffset(StyleUnit::Px(3.0)),
            OutlineFill(Paint::Solid(RED)),
        ),
    );
    // Without a fill, nothing is stroked.
    let unfilled = ui.create(
        Node::Box,
        (fixed(40.0, 20.0), OutlineWidth(StyleUnit::Px(2.0))),
    );
    ui.append(root, outlined);
    ui.append(root, unfilled);

    let mut f = Fixture::default();
    f.build(&ui, root);

    // Stroked along the box moved out by the offset and half the width.
    assert_eq!(
        strokes(&f.draw(&ui)),
        [(2.0, Rect::new(6.0, 6.0, 54.0, 34.0))]
    );
}

#[test]
fn outlines_of_split_fragments_round_all_four_corners() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            BorderRadius(LayoutRect::new_all(StyleUnit::Px(4.0))),
            OutlineWidth(StyleUnit::Px(2.0)),
            OutlineFill(Paint::Solid(RED)),
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

    let scene = f.draw(&ui);
    let outlines: Vec<_> = scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::Stroke(stroke) => Some(&stroke.shape),
            _ => None,
        })
        .collect();
    let fragments: Vec<_> = f.fragments.fragments(f.item(span)).collect();

    assert!(fragments.len() > 1);
    assert_eq!(outlines.len(), fragments.len());

    // Every corner of every fragment's outline is rounded, also at the
    // joins, grown by half the outline width.
    for (outline, key) in outlines.into_iter().zip(fragments) {
        let size = f.fragments.items[key].layout.size;
        let expected = Rect::from_origin_size(Point::ORIGIN, (size.width, size.height))
            .inflate(1.0, 1.0)
            .to_rounded_rect(5.0)
            .to_path(scene.tolerance);

        assert_eq!(outline.elements().len(), expected.elements().len());
        assert!(
            outline
                .elements()
                .iter()
                .zip(expected.elements())
                .all(|(actual, expected)| format!("{actual:?}") == format!("{expected:?}"))
        );
    }
}

impl Fixture {
    /// Paint the whole tree at a display scale factor, into a physical
    /// viewport scaled with it.
    fn draw_scaled(&self, ui: &Ui, scale_factor: f64) -> Scene {
        let mut scene = Scene::new();
        let (layers, root) = self.layers();
        CometPainter::new().draw(
            ui,
            &self.inputs,
            &self.fragments,
            &self.pre_paint,
            &layers,
            root,
            SCENE_VIEWPORT.scale_from_origin(scale_factor),
            scale_factor,
            &mut scene,
        );

        scene
    }
}

/// Glyphs painted, as their font size and scene position.
fn scene_glyphs(scene: &Scene) -> Vec<(f32, Point)> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::GlyphRun(run) => Some(run),
            _ => None,
        })
        .flat_map(|run| {
            run.glyphs.iter().map(|glyph| {
                (
                    run.font_size,
                    run.transform * Point::new(glyph.x as f64, glyph.y as f64),
                )
            })
        })
        .collect()
}

#[test]
fn paints_logical_layouts_at_the_display_scale_factor() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (bx::Padding(LayoutRect::new_all(StyleUnit::Px(5.0))),),
    );
    let block = ui.create(Node::Box, (fixed(40.0, 20.0), Fill(Paint::Solid(RED))));
    ui.append(root, block);
    text(&mut ui, root, "scaled text");

    let mut f = Fixture::default();
    f.build(&ui, root);

    let logical = f.draw_scaled(&ui, 1.0);
    let physical = f.draw_scaled(&ui, 2.0);

    // Boxes paint in physical pixels.
    let fill = physical
        .commands
        .iter()
        .find_map(|command| match command {
            RenderCommand::Fill(fill) => Some(fill),
            _ => None,
        })
        .unwrap();
    assert_affine(
        fill.transform,
        Affine::scale(2.0) * Affine::translate((5.0, 5.0)),
    );
    assert_eq!(
        fill.transform
            .transform_rect_bbox(fill.shape.bounding_box()),
        Rect::new(10.0, 10.0, 90.0, 50.0)
    );

    // Glyphs draw at their physical font size, in the same places scaled.
    let logical = scene_glyphs(&logical);
    let physical = scene_glyphs(&physical);
    assert!(!logical.is_empty());
    assert_eq!(logical.len(), physical.len());

    for ((logical_size, logical), (physical_size, physical)) in logical.into_iter().zip(physical) {
        assert_eq!(physical_size, logical_size * 2.0);
        assert!((physical - logical.to_vec2() * 2.0).to_vec2().hypot() < 1e-3);
    }
}

#[test]
fn invalid_scale_factors_paint_nothing() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (fixed(40.0, 20.0), Fill(Paint::Solid(RED))));

    let mut f = Fixture::default();
    f.build(&ui, root);

    for scale_factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(f.draw_scaled(&ui, scale_factor).commands.is_empty());
    }
}

impl Fixture {
    fn resolve_changed(&mut self, ui: &Ui, root: FragmentKey) {
        self.resolve(ui, root).unwrap();
    }

    /// The pre-paint tree of `root`, resolved from scratch.
    fn full_resolve(&self, ui: &Ui, root: FragmentKey) -> PrePaintTree {
        let mut full = PrePaintTree::new();
        PrePaintTreeResolver::new()
            .resolve(ui, &self.inputs, &self.fragments, &UNITS, &mut full, root)
            .unwrap();

        full
    }

    fn assert_matches_full_resolve(&self, ui: &Ui, root: FragmentKey) {
        let full = self.full_resolve(ui, root);

        for key in self.fragments.items.descendants(root) {
            let source = self.fragments.items[key].source;

            // Generations differ by design: only the values must match.
            let (data, expected) = (self.pre_paint.fragment(key), full.fragment(key));
            assert!(
                data.zip(expected)
                    .is_some_and(|(data, expected)| data == expected),
                "{key:?}: {data:?} != {expected:?}"
            );
            assert_eq!(self.pre_paint.get(source), full.get(source), "{source:?}");
        }
    }
}

#[test]
fn incremental_resolves_follow_changed_fragments_only() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let top = ui.create(Node::Box, (fixed(120.0, 20.0),));
    let middle = ui.create(
        Node::Box,
        (
            fixed(120.0, 30.0),
            BorderRadius(LayoutRect::new_all(StyleUnit::Px(4.0))),
        ),
    );
    let bottom = ui.create(Node::Box, (fixed(120.0, 40.0),));
    ui.append(root, top);
    ui.append(root, middle);
    ui.append(root, bottom);
    text(&mut ui, top, "top");
    text(&mut ui, bottom, "bottom");

    let mut f = Fixture::default();
    let fragment_root = f.layout(&ui, root);
    f.resolve_changed(&ui, fragment_root);
    f.assert_matches_full_resolve(&ui, fragment_root);

    // A taller top box moves the boxes below it.
    ui.set_props(top, (fixed(120.0, 50.0),));
    f.inputs.mark_node(top, InputFlags::NEED_BOX_LAYOUT);
    f.layout(&ui, root);
    f.resolve_changed(&ui, fragment_root);
    f.assert_matches_full_resolve(&ui, fragment_root);
    assert_affine(f.boxed(middle).transform, Affine::translate((0.0, 50.0)));

    // Boxes that did not move or resize are skipped, so an unmarked paint
    // style edit on one is not seen.
    let radius = |data: &FragmentData| data.border_radius;
    let before = radius(f.boxed(middle));
    ui.set_props(
        middle,
        (BorderRadius(LayoutRect::new_all(StyleUnit::Px(8.0))),),
    );
    ui.set_props(bottom, (fixed(120.0, 60.0),));
    f.inputs.mark_node(bottom, InputFlags::NEED_BOX_LAYOUT);
    f.layout(&ui, root);
    f.resolve_changed(&ui, fragment_root);

    assert_eq!(radius(f.boxed(middle)), before);
    assert_ne!(
        f.full_resolve(&ui, fragment_root)
            .fragment(f.box_of(middle))
            .unwrap()
            .border_radius,
        before
    );
    assert_eq!(
        f.pre_paint.get(f.item(bottom)),
        f.full_resolve(&ui, fragment_root).get(f.item(bottom))
    );
}

#[test]
fn resolves_both_changed_and_marked_fragments() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let top = ui.create(Node::Box, (fixed(120.0, 20.0),));
    let wrapper = ui.create(Node::Box, (fixed(120.0, 40.0),));
    let marked = ui.create(Node::Box, (fixed(60.0, 20.0),));
    ui.append(root, top);
    ui.append(root, wrapper);
    ui.append(wrapper, marked);

    let mut f = Fixture::default();
    let fragment_root = f.build(&ui, root);

    // A paint style edit, marked by hand, and a layout change moving another
    // box, resolved together.
    ui.set_props(
        marked,
        (BorderRadius(LayoutRect::new_all(StyleUnit::Px(6.0))),),
    );
    ui.set_props(top, (fixed(120.0, 30.0),));
    f.inputs.mark_node(top, InputFlags::NEED_BOX_LAYOUT);
    f.layout(&ui, root);
    f.mark(marked);

    let pre_paint = InputFlags::NEED_PRE_PAINT;
    assert_eq!(f.inputs.items[f.item(marked)].flags, pre_paint);
    assert_eq!(f.inputs.items[f.item(wrapper)].child_flags, pre_paint);
    assert!(f.inputs.items[f.item(root)].child_flags.contains(pre_paint));

    f.resolve_changed(&ui, fragment_root);
    f.assert_matches_full_resolve(&ui, fragment_root);
    assert_affine(f.boxed(wrapper).transform, Affine::translate((0.0, 30.0)));
    assert!(f.boxed(marked).border_radius.is_some());

    // The resolver keeps the flags for the caller to clear.
    assert_eq!(f.inputs.items[f.item(marked)].flags, pre_paint);
    f.clear_flags();
    assert!(f.inputs.items[f.item(marked)].flags.is_empty());
    assert!(f.inputs.items[f.item(root)].child_flags.is_empty());
}

#[test]
fn marks_kept_by_item_reach_every_fragment_set_before_the_layout() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let paragraph = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let other = ui.create(Node::Box, (fixed(40.0, 10.0),));
    ui.append(root, paragraph);
    ui.append(paragraph, span);
    text(
        &mut ui,
        span,
        "one two three four five six seven eight nine ten",
    );
    ui.append(root, other);

    let mut f = Fixture::default();
    f.build(&ui, root);
    let fragments: Vec<_> = f.fragments.fragments(f.item(span)).collect();
    assert!(fragments.len() > 1);

    // An inline element has no box fragment to mark. Its item is marked
    // before a layout that leaves its lines in place.
    ui.set_props(
        span,
        (BorderRadius(LayoutRect::new_all(StyleUnit::Px(2.0))),),
    );
    f.mark(span);
    ui.set_props(other, (fixed(40.0, 20.0),));
    f.inputs.mark_node(other, InputFlags::NEED_BOX_LAYOUT);
    let fragment_root = f.layout(&ui, root);

    // The layout kept the span's fragments, and the mark still reaches all
    // of them.
    assert_eq!(
        f.fragments.fragments(f.item(span)).collect::<Vec<_>>(),
        fragments
    );
    f.resolve(&ui, fragment_root).unwrap();
    f.clear_flags();

    for key in fragments {
        assert!(f.fragment(key).border_radius.is_some());
    }
    assert!(f.inputs.items[f.item(span)].flags.is_empty());
}
