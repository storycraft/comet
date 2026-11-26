use anyrender::Paint;
use anyrender::{Scene, recording::RenderCommand};
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{BoxFragmentKey, BoxFragmentType, FragmentRef, FragmentTree, FragmentTreeBuilder},
    input::{
        LayoutInputKey,
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
    style::{Opacity, OutlineFill},
};
use comet_paint::{
    prepaint::{
        FragmentData, LayoutInputData,
        builder::PrepaintTreeBuilder,
        fragment::{for_each_paint_child, paint_parent},
        tree::PrepaintTree,
    },
    style::{
        BorderRadius, Fill, OutlineOffset, OutlineWidth, ScrollLeft, ScrollTop, TextColor,
        Transform, TransformOrigin,
    },
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use kurbo::{Affine, Point, RoundedRectRadii, Vec2};
use kurbo::{Rect, Shape};
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
    dirty: DirtyMap,
    fragments: FragmentTree,
    prepaint: PrepaintTree,
    resolver: PrepaintTreeBuilder,
    input_builder: LayoutInputTreeBuilder,
    builder: FragmentTreeBuilder,
    root: Option<BoxFragmentKey>,
    input_root: Option<LayoutInputKey>,
}

impl Fixture {
    /// Build the layout, resolve the pre-paint tree and clear the flags.
    fn build(&mut self, ui: &Ui, node: NodeKey) -> BoxFragmentKey {
        let root = self.layout(ui, node);
        self.resolve(ui, root).unwrap();
        self.clear_flags();

        root
    }

    /// Clear the input flags, once the pre-paint tree was resolved.
    fn clear_flags(&mut self) {
        if let Some(input_root) = self.input_root {
            self.dirty.clear(&self.inputs, input_root);
        }
    }

    /// Build or update the layout only, keeping the resolved pre-paint tree
    /// and the input flags for the next resolve.
    fn layout(&mut self, ui: &Ui, node: NodeKey) -> BoxFragmentKey {
        let input_root = self
            .input_builder
            .build(
                ui,
                &mut self.inputs,
                &mut self.dirty,
                BuildRequest::new(node, 16.0, VIEWPORT),
            )
            .unwrap();
        let root = self
            .builder
            .build(
                ui,
                &self.inputs,
                &mut self.dirty,
                input_root,
                &mut self.fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0),
            )
            .unwrap();
        self.input_root = Some(input_root);
        self.root = Some(root);

        root
    }

    fn resolve(&mut self, ui: &Ui, key: BoxFragmentKey) -> Option<()> {
        self.resolver.build(
            ui,
            &self.inputs,
            &mut self.dirty,
            &self.fragments,
            &UNITS,
            &mut self.prepaint,
            key,
            false,
        )
    }

    /// Mark the item of a node as needing pre-paint.
    fn mark(&mut self, node: NodeKey) {
        self.dirty
            .mark_node(&self.inputs, node, DirtyFlags::NEED_PRE_PAINT);
    }

    fn item(&self, node: NodeKey) -> LayoutInputKey {
        self.inputs.item_for_node(node).unwrap()
    }

    fn box_of(&self, node: NodeKey) -> BoxFragmentKey {
        self.fragments.box_fragment(self.item(node)).unwrap()
    }

    fn data(&self, node: NodeKey) -> &LayoutInputData {
        self.prepaint.get(self.item(node)).unwrap()
    }

    fn border_radius(&self, key: BoxFragmentKey) -> Option<RoundedRectRadii> {
        self.prepaint.fragment(key)?.border_radius
    }

    /// The border radii of a fragment, square at its joins, or `None` if it
    /// has no border radius.
    fn border_radii(&self, key: BoxFragmentKey) -> Option<RoundedRectRadii> {
        let data = self.prepaint.fragment(key)?;
        data.border_radius?;

        Some(data.border_radii(&self.fragments.boxes[key]))
    }

    fn fragment(&self, key: BoxFragmentKey) -> &FragmentData {
        self.prepaint.fragment(key).unwrap()
    }

    /// The data of a node's box fragment.
    fn boxed(&self, node: NodeKey) -> &FragmentData {
        self.fragment(self.box_of(node))
    }

    fn clip(&self, node: NodeKey) -> (bool, bool) {
        self.data(node).clip
    }

    /// The scroll offset a node's box applies to its descendants, from the
    /// layers built for the whole fragment tree.
    fn scroll(&self, ui: &Ui, node: NodeKey) -> Vec2 {
        let (layers, _) = self.layers(ui);

        layers.scroll_offset(self.box_of(node))
    }

    /// The transform of a fragment relative to its parent.
    fn transform(&self, key: BoxFragmentKey) -> Affine {
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

    /// Build the paint layers of the whole fragment tree, with their root.
    fn layers(&self, ui: &Ui) -> (PaintLayerTree, PaintLayerKey) {
        let mut layers = PaintLayerTree::new();
        let root = PaintLayerBuilder::new()
            .build(
                ui,
                &self.inputs,
                &self.dirty,
                &self.fragments,
                &mut layers,
                self.input_root.unwrap(),
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
            &self.prepaint,
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

    let block_fragment = f.fragment(f.box_of(block));
    assert_eq!(
        block_fragment.inverse_transform,
        Some(Affine::translate((-5.0, -5.0)))
    );
    assert_eq!(f.clip(block), (false, false));

    // Glyph runs are inline items, without data of their own.
    let runs: Vec<_> = f.fragments.fragments(&f.inputs, f.item(content)).collect();
    assert!(!runs.is_empty());
    assert!(
        runs.iter()
            .all(|run| matches!(run, FragmentRef::Inline { .. }))
    );
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

    let fragments: Vec<_> = f.fragments.inline_boxes(&f.inputs, f.item(span)).collect();
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
    let width = f.fragments.boxes[first].layout.geometry.size.width;
    assert!(
        f.prepaint
            .hit_local(&f.fragments, first, Point::new(width - 0.5, 0.5))
    );
    assert!(
        !f.prepaint
            .hit_local(&f.fragments, first, Point::new(0.5, 0.5))
    );

    // Percentages resolve against the smaller side of each fragment.
    assert_eq!(
        f.border_radius(f.box_of(relative)),
        Some(RoundedRectRadii::from(10.0))
    );

    let split_fragments: Vec<_> = f.fragments.inline_boxes(&f.inputs, f.item(split)).collect();
    assert!(split_fragments.len() > 1);
    for (index, &fragment) in split_fragments.iter().enumerate() {
        let size = f.fragments.boxes[fragment].layout.geometry.size;
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
    let root = ui.create(
        Node::Box,
        (BorderRadius(LayoutRect::new_all(StyleUnit::Px(2.0))),),
    );
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

    // The block splits the root's inline content into runs, which carry
    // the root's item but paint nothing, so have no radius.
    assert_eq!(
        f.border_radius(root_fragment),
        Some(RoundedRectRadii::from(2.0))
    );
    let runs: Vec<_> = f.fragments.runs(root_fragment).collect();
    assert!(!runs.is_empty());
    for run in runs {
        assert_eq!(f.fragments.boxes[run].box_type, BoxFragmentType::Anonymous);
        assert_eq!(f.border_radius(run), None);
    }

    // Units resolve with the item's font size, percentages against the
    // smaller side.
    assert_eq!(
        f.border_radius(f.box_of(rounded)),
        Some(RoundedRectRadii::new(3.0, 4.0, 8.0, 10.0))
    );

    // Only the outer corners of wrapped fragments' borders are rounded.
    let fragments: Vec<_> = f.fragments.inline_boxes(&f.inputs, f.item(span)).collect();
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
    // A text node's own color applies to its glyph runs, but not to its
    // siblings.
    let own = text(&mut ui, span, "blue ");
    ui.set_props(own, (TextColor(Paint::Solid(BLUE)),));
    text(&mut ui, span, "red again ");
    let green = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, TextColor(Paint::Solid(GREEN))),
    );
    ui.append(span, green);
    text(&mut ui, green, "green");

    let mut f = Fixture::default();
    let root_fragment = f.build(&ui, root);

    assert_eq!(
        f.fragments.boxes.parent(f.box_of(block)),
        Some(root_fragment)
    );
    // Blue, then red before, inside and after the block, the text node's
    // own blue, red again, then green.
    assert_eq!(f.text_colors(&ui), [BLUE, RED, BLUE, RED, GREEN]);
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

    assert_affine(f.boxed(outer).transform, outer_transform);
    // A translation is the same around any origin. Ancestor transforms are
    // not included.
    assert_affine(f.boxed(inner).transform, Affine::translate((5.0, 0.0)));

    // Placed at its location in the outer box, then scaled around its corner.
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
    f.dirty
        .mark_node(&f.inputs, top, DirtyFlags::NEED_BOX_LAYOUT);
    f.layout(&ui, root);

    // Resolved data is kept until it is resolved again.
    assert_affine(
        f.transform(clipper_fragment),
        Affine::translate((3.0, 10.0)),
    );

    // Resolving follows the moved and resized fragments.
    let root_fragment = f.root.unwrap();
    f.resolve(&ui, root_fragment).unwrap();

    assert_affine(
        f.transform(clipper_fragment),
        Affine::translate((3.0, 30.0)),
    );
    // The root grew, but it stays in place.
    assert_eq!(f.boxed(root).transform, root_data.transform);
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
    f.dirty
        .mark_node(&f.inputs, root, DirtyFlags::NEED_CHILDREN);
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
    let hit = |key, x, y| f.prepaint.hit_local(&f.fragments, key, Point::new(x, y));

    // Local coordinates start at the border box corner, whatever its layout
    // location.
    assert!(hit(rounded, 20.0, 10.0));
    assert!(hit(rounded, 0.0, 10.0));
    assert!(!hit(rounded, 1.0, 1.0));
    assert!(!hit(rounded, 41.0, 10.0));
    assert!(!hit(rounded, -1.0, 10.0));

    // Anonymous boxes are hit inside their box too.
    let run = f.fragments.runs(root_fragment).next().unwrap();
    assert!(hit(run, 1.0, 1.0));
    assert!(!hit(run, -1.0, 1.0));

    // Fragments missing from the trees are not hit instead of panicking.
    assert!(!hit(BoxFragmentKey::default(), 1.0, 1.0));
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
    assert_eq!(f.boxed(parent).transform.as_coeffs()[0], 2.0);
    assert_eq!(f.clip(child), (false, false));
    assert_eq!(f.boxed(child), &child_data);

    // Marking the child reaches it.
    f.mark(child);
    f.resolve(&ui, f.root.unwrap()).unwrap();

    assert_eq!(f.clip(child), (true, true));
    assert_eq!(f.boxed(child).transform.as_coeffs()[0], 3.0);
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
    let fragments: Vec<_> = f.fragments.inline_boxes(&f.inputs, f.item(span)).collect();

    assert!(fragments.len() > 1);
    assert_eq!(outlines.len(), fragments.len());

    // Every corner of every fragment's outline is rounded, also at the
    // joins, grown by half the outline width.
    for (outline, key) in outlines.into_iter().zip(fragments) {
        let size = f.fragments.boxes[key].layout.geometry.size;
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
        let (layers, root) = self.layers(ui);
        CometPainter::new().draw(
            ui,
            &self.inputs,
            &self.fragments,
            &self.prepaint,
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
    fn resolve_changed(&mut self, ui: &Ui, root: BoxFragmentKey) {
        self.resolve(ui, root).unwrap();
    }

    /// The pre-paint tree of `root`, resolved from scratch. Its marks go to
    /// a copy of the flags, which are left as they are.
    fn full_resolve(&self, ui: &Ui, root: BoxFragmentKey) -> PrepaintTree {
        let mut full = PrepaintTree::new();
        PrepaintTreeBuilder::new()
            .build(
                ui,
                &self.inputs,
                &mut self.dirty.clone(),
                &self.fragments,
                &UNITS,
                &mut full,
                root,
                false,
            )
            .unwrap();

        full
    }

    fn assert_matches_full_resolve(&self, ui: &Ui, root: BoxFragmentKey) {
        let full = self.full_resolve(ui, root);

        for key in self.fragments.boxes.descendants(root) {
            let source = self.fragments.boxes[key].source;

            // Generations differ by design: only the values must match.
            let (data, expected) = (self.prepaint.fragment(key), full.fragment(key));
            assert!(
                data.zip(expected)
                    .is_some_and(|(data, expected)| data == expected),
                "{key:?}: {data:?} != {expected:?}"
            );
            assert_eq!(self.prepaint.get(source), full.get(source), "{source:?}");
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
    f.clear_flags();

    // A taller top box moves the boxes below it, which are not marked.
    ui.set_props(top, (fixed(120.0, 50.0),));
    f.dirty
        .mark_node(&f.inputs, top, DirtyFlags::NEED_BOX_LAYOUT);
    f.layout(&ui, root);
    f.resolve_changed(&ui, fragment_root);
    f.assert_matches_full_resolve(&ui, fragment_root);
    assert_affine(f.boxed(middle).transform, Affine::translate((0.0, 50.0)));
    f.clear_flags();

    // Boxes that did not move or resize are skipped, so an unmarked paint
    // style edit on one is not seen.
    let radius = |data: &FragmentData| data.border_radius;
    let before = radius(f.boxed(middle));
    ui.set_props(
        middle,
        (BorderRadius(LayoutRect::new_all(StyleUnit::Px(8.0))),),
    );
    ui.set_props(bottom, (fixed(120.0, 60.0),));
    f.dirty
        .mark_node(&f.inputs, bottom, DirtyFlags::NEED_BOX_LAYOUT);
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
        f.prepaint.get(f.item(bottom)),
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
    f.dirty
        .mark_node(&f.inputs, top, DirtyFlags::NEED_BOX_LAYOUT);
    f.layout(&ui, root);
    f.mark(marked);

    let prepaint = DirtyFlags::NEED_PRE_PAINT;
    assert_eq!(f.dirty.entry(f.item(marked)).flags, prepaint);
    assert_eq!(f.dirty.entry(f.item(wrapper)).child_bits, prepaint);
    assert!(f.dirty.entry(f.item(root)).child_bits.contains(prepaint));

    f.resolve_changed(&ui, fragment_root);
    f.assert_matches_full_resolve(&ui, fragment_root);
    assert_affine(f.boxed(wrapper).transform, Affine::translate((0.0, 30.0)));
    assert!(f.boxed(marked).border_radius.is_some());

    // The resolver keeps the flags for the caller to clear, and marks the
    // layers of the fragments it changed.
    assert_eq!(
        f.dirty.entry(f.item(marked)).flags,
        prepaint | DirtyFlags::NEED_LAYER
    );
    f.clear_flags();
    assert!(f.dirty.entry(f.item(marked)).flags.is_empty());
    assert!(f.dirty.entry(f.item(root)).child_bits.is_empty());
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
    let fragments: Vec<_> = f.fragments.inline_boxes(&f.inputs, f.item(span)).collect();
    assert!(fragments.len() > 1);

    // An inline element has no box fragment to mark. Its item is marked
    // before a layout that leaves its lines in place.
    ui.set_props(
        span,
        (BorderRadius(LayoutRect::new_all(StyleUnit::Px(2.0))),),
    );
    f.mark(span);
    ui.set_props(other, (fixed(40.0, 20.0),));
    f.dirty
        .mark_node(&f.inputs, other, DirtyFlags::NEED_BOX_LAYOUT);
    let fragment_root = f.layout(&ui, root);

    // The layout kept the span's fragments, and the mark still reaches all
    // of them.
    assert_eq!(
        f.fragments
            .inline_boxes(&f.inputs, f.item(span))
            .collect::<Vec<_>>(),
        fragments
    );
    f.resolve(&ui, fragment_root).unwrap();
    f.clear_flags();

    for key in fragments {
        assert!(f.fragment(key).border_radius.is_some());
    }
    assert!(f.dirty.entry(f.item(span)).flags.is_empty());
}

#[test]
fn viewport_changes_resolve_viewport_dependent_paint_values_again() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let relative = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            BorderRadius(LayoutRect::new_all(StyleUnit::Vw(0.1))),
            OutlineWidth(StyleUnit::Vw(0.05)),
        ),
    );
    let absolute = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            BorderRadius(LayoutRect::new_all(StyleUnit::Px(3.0))),
        ),
    );
    ui.append(root, relative);
    ui.append(root, absolute);

    let mut f = Fixture::default();
    let fragment_root = f.build(&ui, root);
    let radius = |f: &Fixture, node| f.boxed(node).border_radius.unwrap().top_left;

    // 10vw and 5vw of the 120px wide viewport.
    assert_eq!(radius(&f, relative), 12.0);
    assert_eq!(f.data(relative).outline_width, 6.0);
    assert!(f.prepaint.depends_viewport(f.item(relative)));
    assert!(!f.prepaint.depends_viewport(f.item(absolute)));
    assert!(!f.prepaint.depends_viewport(f.item(root)));

    let wide = LayoutConstraint::new_root(Size::new(200.0, 400.0), 16.0).units;
    let resolve = |f: &mut Fixture, ui: &Ui, units, viewport_changed| {
        f.resolver
            .build(
                ui,
                &f.inputs,
                &mut f.dirty,
                &f.fragments,
                units,
                &mut f.prepaint,
                fragment_root,
                viewport_changed,
            )
            .unwrap();
    };

    // Without the change, nothing is resolved again.
    resolve(&mut f, &ui, &wide, false);
    assert_eq!(radius(&f, relative), 12.0);

    // With it, only the dependents are, against the wider viewport.
    let absolute_data = f.boxed(absolute).clone();
    resolve(&mut f, &ui, &wide, true);
    assert_eq!(radius(&f, relative), 20.0);
    assert_eq!(f.data(relative).outline_width, 10.0);
    assert_eq!(f.boxed(absolute), &absolute_data);
    assert!(f.prepaint.depends_viewport(f.item(relative)));

    // Dropping the viewport units stops tracking the item.
    ui.set_props(
        relative,
        (
            BorderRadius(LayoutRect::new_all(StyleUnit::Px(2.0))),
            OutlineWidth(StyleUnit::Px(1.0)),
        ),
    );
    f.mark(relative);
    resolve(&mut f, &ui, &wide, false);
    f.clear_flags();
    assert_eq!(radius(&f, relative), 2.0);
    assert!(!f.prepaint.depends_viewport(f.item(relative)));

    resolve(&mut f, &ui, &UNITS, true);
    assert_eq!(radius(&f, relative), 2.0);
}

#[test]
fn font_size_changes_resolve_em_paint_values_again() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    // A fixed size, so the font size moves and resizes nothing.
    let rounded = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            BorderRadius(LayoutRect::new_all(StyleUnit::Em(0.25))),
            OutlineWidth(StyleUnit::Em(0.125)),
        ),
    );
    ui.append(root, rounded);

    let mut f = Fixture::default();
    let fragment_root = f.build(&ui, root);
    let radius = |f: &Fixture| f.boxed(rounded).border_radius.unwrap().top_left;
    assert_eq!(radius(&f), 4.0);
    assert_eq!(f.data(rounded).outline_width, 2.0);

    ui.set_props(rounded, (bx::FontSize(StyleUnit::Px(32.0)),));
    f.dirty
        .mark_node(&f.inputs, rounded, DirtyFlags::NEED_FULL_LAYOUT);
    let laid_out = f.layout(&ui, root);
    assert_eq!(laid_out, fragment_root);
    f.resolve(&ui, fragment_root).unwrap();

    assert_eq!(radius(&f), 8.0);
    assert_eq!(f.data(rounded).outline_width, 4.0);
    f.assert_matches_full_resolve(&ui, fragment_root);
}

#[test]
fn hiding_overflow_on_one_axis_clips_both() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let scroller = ui.create(
        Node::Box,
        (bx::Overflow(LayoutPoint::new(
            Overflow::Hidden,
            Overflow::Visible,
        )),),
    );
    let scrolled = ui.create(
        Node::Box,
        (bx::Overflow(LayoutPoint::new(
            Overflow::Visible,
            Overflow::Scroll,
        )),),
    );
    let clipped = ui.create(
        Node::Box,
        (bx::Overflow(LayoutPoint::new(
            Overflow::Visible,
            Overflow::Clip,
        )),),
    );
    ui.append(root, scroller);
    ui.append(root, scrolled);
    ui.append(root, clipped);

    let mut f = Fixture::default();
    f.build(&ui, root);

    // Like Taffy's layout overflow, which they bound.
    assert_eq!(f.clip(scroller), (true, true));
    assert_eq!(f.clip(scrolled), (true, true));
    assert_eq!(f.clip(clipped), (false, true));
}

#[test]
fn scroll_resolves_only_for_scroll_containers() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let scroll = |overflow: Option<bx::Overflow>, left: f64, top: f64| {
        let mut builder = comet_tree::builder::NodeBuilder::new();
        builder.add_bundle((fixed(10.0, 10.0), ScrollLeft(left), ScrollTop(top)));
        if let Some(overflow) = overflow {
            builder.add(overflow);
        }
        builder
    };
    let cases = [
        scroll(Some(hidden()), 3.0, 7.0),
        // Scrolling one axis makes both scroll.
        scroll(
            Some(bx::Overflow(LayoutPoint::new(
                Overflow::Visible,
                Overflow::Scroll,
            ))),
            3.0,
            7.0,
        ),
        scroll(
            Some(bx::Overflow(LayoutPoint::new(
                Overflow::Clip,
                Overflow::Clip,
            ))),
            3.0,
            7.0,
        ),
        scroll(None, 3.0, 7.0),
        scroll(Some(hidden()), -4.0, f64::NAN),
    ];
    let nodes = cases.map(|mut builder| {
        let key = ui.reserve_node();
        let node = ui.create_prebuilt(key, builder.build(Node::Box)).unwrap();
        // 20x20 content overflows the 10x10 box by 10 on both axes.
        let content = ui.create(Node::Box, (fixed(20.0, 20.0),));
        ui.append(root, node);
        ui.append(node, content);
        node
    });

    let mut f = Fixture::default();
    f.build(&ui, root);

    let scrolls = nodes.map(|node| f.scroll(&ui, node));
    assert_eq!(
        scrolls,
        [
            Vec2::new(3.0, 7.0),
            Vec2::new(3.0, 7.0),
            Vec2::ZERO,
            Vec2::ZERO,
            Vec2::ZERO,
        ]
    );
    assert_eq!(f.scroll(&ui, root), Vec2::ZERO);
}

#[test]
fn scroll_is_clamped_to_the_overflowing_content() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let scroller = ui.create(
        Node::Box,
        (
            fixed(50.0, 30.0),
            bx::Border(LayoutRect::new_all(StyleUnit::Px(2.0))),
            hidden(),
            ScrollLeft(1000.0),
            ScrollTop(10.0),
        ),
    );
    let tall = ui.create(Node::Box, (fixed(40.0, 100.0),));
    ui.append(root, scroller);
    ui.append(scroller, tall);

    let mut f = Fixture::default();
    let root_fragment = f.build(&ui, root);

    // Nothing overflows horizontally; 10 is within the vertical range.
    assert_eq!(f.scroll(&ui, scroller), Vec2::new(0.0, 10.0));

    // The content, 100 high from the padding box top, overflows the 26 high
    // padding box by 74.
    ui.set_props(scroller, (ScrollTop(1000.0),));
    f.mark(scroller);
    f.resolve(&ui, root_fragment).unwrap();
    f.clear_flags();

    assert_eq!(f.scroll(&ui, scroller), Vec2::new(0.0, 74.0));

    // Shrinking the content changes the container's overflow but not its
    // size, which marks its layer to resolve the clamped scroll again.
    ui.set_props(tall, (fixed(40.0, 50.0),));
    f.dirty
        .mark_node(&f.inputs, tall, DirtyFlags::NEED_BOX_LAYOUT);
    f.layout(&ui, root);
    assert!(
        f.dirty
            .entry(f.item(scroller))
            .flags
            .contains(DirtyFlags::NEED_LAYER)
    );
    f.resolve(&ui, root_fragment).unwrap();
    f.clear_flags();

    assert_eq!(f.scroll(&ui, scroller), Vec2::new(0.0, 24.0));
    f.assert_matches_full_resolve(&ui, root_fragment);
}

#[test]
fn scroll_changes_resolve_without_layout() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let scroller = ui.create(Node::Box, (fixed(50.0, 30.0), hidden()));
    let tall = ui.create(Node::Box, (fixed(50.0, 100.0),));
    ui.append(root, scroller);
    ui.append(scroller, tall);

    let mut f = Fixture::default();
    let root_fragment = f.build(&ui, root);
    assert_eq!(f.scroll(&ui, scroller), Vec2::ZERO);

    ui.set_props(scroller, (ScrollTop(5.0),));
    f.mark(scroller);
    f.resolve(&ui, root_fragment).unwrap();

    assert_eq!(f.scroll(&ui, scroller), Vec2::new(0.0, 5.0));
    f.assert_matches_full_resolve(&ui, root_fragment);
}

/// A root with 5px padding holding a 50x30 scroll container with a 2px
/// border, scrolled by `scroll`, with `extra` props on the container, and
/// three 40x20 rows inside. Returns the UI, the root, the container and
/// the rows.
fn scroll_fixture(
    scroll: (ScrollLeft, ScrollTop),
    extra: impl comet_tree::style::StyleProps,
) -> (Ui, NodeKey, NodeKey, [NodeKey; 3]) {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (bx::Padding(LayoutRect::new_all(StyleUnit::Px(5.0))),),
    );
    let scroller = ui.create(
        Node::Box,
        (
            fixed(50.0, 30.0),
            bx::Border(LayoutRect::new_all(StyleUnit::Px(2.0))),
            hidden(),
            Fill(Paint::Solid(RED)),
            scroll.0,
            scroll.1,
        ),
    );
    ui.set_props(scroller, extra);
    ui.append(root, scroller);

    let rows = [GREEN, BLUE, RED].map(|color| {
        let row = ui.create(Node::Box, (fixed(40.0, 20.0), Fill(Paint::Solid(color))));
        ui.append(scroller, row);
        row
    });

    (ui, root, scroller, rows)
}

#[test]
fn scroll_containers_move_their_contents_but_not_themselves() {
    let (ui, root, _, _) = scroll_fixture((ScrollLeft(0.0), ScrollTop(10.0)), ());

    let mut f = Fixture::default();
    f.build(&ui, root);

    // The container and its clip stay; the rows move up by 10. The third
    // row, at 37..57 after scrolling, is outside the clip and skipped.
    assert_eq!(
        commands(&f.draw(&ui)),
        [
            format!("fill {:?}", Rect::new(5.0, 5.0, 55.0, 35.0)),
            format!("clip {:?}", Rect::new(5.0, 5.0, 55.0, 35.0)),
            format!("fill {:?}", Rect::new(7.0, -3.0, 47.0, 17.0)),
            format!("fill {:?}", Rect::new(7.0, 17.0, 47.0, 37.0)),
            "pop".into(),
        ]
    );
}

#[test]
fn painted_scroll_is_clamped_to_the_scrollable_range() {
    let (ui, root, _, _) = scroll_fixture((ScrollLeft(100.0), ScrollTop(1000.0)), ());

    let mut f = Fixture::default();
    f.build(&ui, root);

    // 60 high rows overflow the 26 high padding box by 34; nothing
    // overflows horizontally. Only the last row is in view.
    assert_eq!(
        commands(&f.draw(&ui)),
        [
            format!("fill {:?}", Rect::new(5.0, 5.0, 55.0, 35.0)),
            format!("clip {:?}", Rect::new(5.0, 5.0, 55.0, 35.0)),
            format!("fill {:?}", Rect::new(7.0, -7.0, 47.0, 13.0)),
            format!("fill {:?}", Rect::new(7.0, 13.0, 47.0, 33.0)),
            "pop".into(),
        ]
    );
}

#[test]
fn layer_owners_scroll_their_contents() {
    let (ui, root, _, _) = scroll_fixture((ScrollLeft(0.0), ScrollTop(10.0)), (Opacity(0.5),));

    let mut f = Fixture::default();
    f.build(&ui, root);

    let commands = commands(&f.draw(&ui));
    let fills: Vec<_> = commands
        .iter()
        .filter(|command| command.starts_with("fill"))
        .cloned()
        .collect();

    assert_eq!(
        fills,
        [
            format!("fill {:?}", Rect::new(5.0, 5.0, 55.0, 35.0)),
            format!("fill {:?}", Rect::new(7.0, -3.0, 47.0, 17.0)),
            format!("fill {:?}", Rect::new(7.0, 17.0, 47.0, 37.0)),
        ]
    );
}

#[test]
fn child_layers_scroll_with_their_container() {
    let (mut ui, root, _, rows) = scroll_fixture((ScrollLeft(0.0), ScrollTop(10.0)), ());
    // The second row starts a layer of its own.
    ui.set_props(rows[1], (Transform(Affine::IDENTITY),));

    let mut f = Fixture::default();
    f.build(&ui, root);

    // The layer paints after the contents, inside the container's clip,
    // pushed again for it, and scrolled.
    assert_eq!(
        commands(&f.draw(&ui)),
        [
            format!("fill {:?}", Rect::new(5.0, 5.0, 55.0, 35.0)),
            format!("clip {:?}", Rect::new(5.0, 5.0, 55.0, 35.0)),
            format!("fill {:?}", Rect::new(7.0, -3.0, 47.0, 17.0)),
            "pop".into(),
            format!("clip {:?}", Rect::new(5.0, 5.0, 55.0, 35.0)),
            format!("fill {:?}", Rect::new(7.0, 17.0, 47.0, 37.0)),
            "pop".into(),
        ]
    );
}

#[test]
fn layers_painted_alone_scroll_with_their_ancestors() {
    let (mut ui, root, _, rows) = scroll_fixture((ScrollLeft(0.0), ScrollTop(10.0)), ());
    ui.set_props(rows[1], (Transform(Affine::IDENTITY),));

    let mut f = Fixture::default();
    f.build(&ui, root);

    let (layers, _) = f.layers(&ui);
    let layer = layers.layer_of(f.box_of(rows[1])).unwrap();
    let mut scene = Scene::new();
    CometPainter::new().draw(
        &ui,
        &f.inputs,
        &f.fragments,
        &f.prepaint,
        &layers,
        layer,
        SCENE_VIEWPORT,
        1.0,
        &mut scene,
    );

    // Clips of the ancestors do not apply, but their scroll does.
    assert_eq!(
        commands(&scene),
        [format!("fill {:?}", Rect::new(7.0, 17.0, 47.0, 37.0))]
    );
}

#[test]
fn paint_parents_follow_the_inline_elements_holding_content() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let inner = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let atomic = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            bx::DisplayInner::FlowRoot,
            fixed(10.0, 10.0),
        ),
    );
    ui.append(root, outer);
    text(&mut ui, outer, "outer ");
    ui.append(outer, inner);
    let content = text(&mut ui, inner, "inner");
    ui.append(inner, atomic);

    let mut f = Fixture::default();
    let root_fragment = f.build(&ui, root);
    let part = |node| {
        f.fragments
            .inline_boxes(&f.inputs, f.item(node))
            .next()
            .unwrap()
    };
    let (outer_part, inner_part) = (part(outer), part(inner));

    // Inline part and atomic inline boxes are flat children of the root.
    let atomic_box = f.box_of(atomic);
    for key in [outer_part, inner_part, atomic_box] {
        assert_eq!(f.fragments.boxes.parent(key), Some(root_fragment));
    }

    // In paint order, they are held by the items of the pieces of their
    // elements, which the inline part boxes decorate.
    let piece = |node| {
        f.fragments
            .fragments(&f.inputs, f.item(node))
            .next()
            .unwrap()
    };
    let (outer_piece, inner_piece) = (piece(outer), piece(inner));
    assert!(matches!(outer_piece, FragmentRef::Inline { .. }));

    let parent = |fragment| paint_parent(&f.fragments, fragment);
    assert_eq!(parent(FragmentRef::Box(atomic_box)), Some(inner_piece));
    assert_eq!(parent(inner_piece), Some(outer_piece));
    assert_eq!(parent(outer_piece), Some(FragmentRef::Box(root_fragment)));
    assert_eq!(parent(FragmentRef::Box(root_fragment)), None);

    // An inline part box has the parent of its piece.
    assert_eq!(parent(FragmentRef::Box(inner_part)), Some(outer_piece));

    // Glyph runs too, though they have no box.
    assert_eq!(parent(piece(content)), Some(inner_piece));

    // The paint children of a piece are its items.
    let mut children = Vec::new();
    for_each_paint_child(&f.fragments, inner_piece, |child| children.push(child));
    assert!(children.contains(&FragmentRef::Box(atomic_box)));
    assert!(children.contains(&piece(content)));
    assert!(!children.contains(&FragmentRef::Box(inner_part)));

    // Only boxes have data.
    for key in [outer_part, inner_part, atomic_box] {
        assert!(f.prepaint.fragment(key).is_some());
    }
}
