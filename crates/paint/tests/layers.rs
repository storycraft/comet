use std::sync::Arc;

use anyrender::{Paint, Scene, recording::RenderCommand};
use comet_layout::{
    fragment::{FragmentItem, FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Rect as LayoutRect, Size, StyleUnit, UnitResolveContext},
        taffy::Overflow,
    },
};
use comet_paint::{
    hit::PaintHitTester,
    layer::{PaintLayerEntry, PaintLayerKey, builder::PaintLayerBuilder, tree::PaintLayerTree},
    painter::CometPainter,
    pre_paint::{resolver::PrePaintTreeResolver, tree::PrePaintTree},
    style::{BackdropFilter, BorderRadius, Fill, Filter, Opacity, Transform, ZIndex},
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use kurbo::{Affine, Point, Rect, Shape};
use peniko::Color;

const VIEWPORT: Size<f64> = Size::new(120.0, 400.0);
const SCENE_VIEWPORT: Rect = Rect::new(0.0, 0.0, 120.0, 400.0);
const UNITS: UnitResolveContext = LayoutConstraint::new_root(VIEWPORT, 16.0).units;

const RED: Color = Color::from_rgb8(200, 20, 20);
const GREEN: Color = Color::from_rgb8(20, 160, 20);
const BLUE: Color = Color::from_rgb8(20, 40, 180);

/// Layout, pre-paint and layer trees of one UI root.
struct Fixture {
    inputs: LayoutInputTree,
    fragments: FragmentTree,
    pre_paint: PrePaintTree,
    layers: PaintLayerTree,
    /// Root layer of `layers`.
    root: PaintLayerKey,
}

impl Fixture {
    fn build(ui: &Ui, node: NodeKey) -> Self {
        let mut inputs = LayoutInputTree::new();
        let input_root = LayoutInputTreeBuilder::new()
            .build(ui, &mut inputs, BuildRequest::new(node, 16.0, VIEWPORT))
            .unwrap();
        let mut fragments = FragmentTree::new();
        let root = FragmentTreeBuilder::new()
            .build(
                ui,
                &inputs,
                input_root,
                &mut fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0),
            )
            .unwrap();
        inputs.clear_flags(input_root);

        let mut pre_paint = PrePaintTree::new();
        PrePaintTreeResolver::new()
            .resolve(ui, &inputs, &fragments, &UNITS, &mut pre_paint, root)
            .unwrap();

        let mut layers = PaintLayerTree::new();
        let root = PaintLayerBuilder::new()
            .build(ui, &inputs, &fragments, &mut layers, root)
            .unwrap();

        Self {
            inputs,
            fragments,
            pre_paint,
            layers,
            root,
        }
    }

    fn box_of(&self, node: NodeKey) -> FragmentKey {
        let item = self.inputs.item_for_node(node).unwrap();

        self.fragments.box_fragment(item).unwrap()
    }

    /// The layer owned by a node's box fragment.
    fn layer_of(&self, node: NodeKey) -> PaintLayerKey {
        let owner = self.box_of(node);

        self.layers
            .iter()
            .find(|(_, layer)| layer.owner == owner)
            .map(|(key, _)| key)
            .unwrap()
    }

    /// The entries of a layer.
    fn entries(&self, layer: PaintLayerKey) -> &[PaintLayerEntry] {
        &self.layers[layer].entries
    }

    /// The fragment hit at a scene point.
    fn hit(&self, x: f64, y: f64) -> Option<FragmentKey> {
        PaintHitTester::new().hit(
            &self.layers,
            &self.fragments,
            &self.pre_paint,
            self.root,
            Point::new(x, y),
        )
    }

    fn draw(&self, ui: &Ui) -> Scene {
        let mut scene = Scene::new();
        CometPainter::new().draw(
            ui,
            &self.inputs,
            &self.fragments,
            &self.pre_paint,
            &self.layers,
            self.root,
            SCENE_VIEWPORT,
            &mut scene,
        );

        scene
    }
}

fn fixed(width: f64, height: f64) -> bx::Size {
    bx::Size(Size::new(
        AutoValue::Value(StyleUnit::Px(width)),
        AutoValue::Value(StyleUnit::Px(height)),
    ))
}

fn solid(paint: &Paint) -> Color {
    match paint {
        Paint::Solid(color) => *color,
        _ => panic!("test only uses solid paints"),
    }
}

fn z(value: i32) -> ZIndex {
    ZIndex(AutoValue::Value(value))
}

/// Fill colors, layer alphas, clips and pops of a scene, in order.
fn commands(scene: &Scene) -> Vec<String> {
    scene
        .commands
        .iter()
        .map(|command| match command {
            RenderCommand::Fill(fill) => format!("fill {:?}", solid(&fill.brush).to_rgba8()),
            RenderCommand::PushLayer(layer) => format!("layer {}", layer.alpha),
            RenderCommand::PushClipLayer(clip) => format!(
                "clip {:?}",
                clip.transform.transform_rect_bbox(clip.clip.bounding_box())
            ),
            RenderCommand::PopLayer => "pop".into(),
            _ => "other".into(),
        })
        .collect()
}

fn fill(color: Color) -> String {
    format!("fill {:?}", color.to_rgba8())
}

#[test]
fn fragments_without_layer_styles_share_the_root_layer() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, (fixed(40.0, 20.0),));
    let inner = ui.create(Node::Box, (fixed(10.0, 10.0), ZIndex(AutoValue::Auto)));
    ui.append(root, outer);
    ui.append(outer, inner);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.layers.len(), 1);
    assert_eq!(f.layers[f.root].owner, f.box_of(root));
    assert_eq!(
        f.entries(f.root),
        [
            PaintLayerEntry::Fragment(f.box_of(root)),
            PaintLayerEntry::Fragment(f.box_of(outer)),
            PaintLayerEntry::Fragment(f.box_of(inner)),
        ]
    );
}

#[test]
fn transforms_opacity_and_z_indices_start_layers() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let transformed = ui.create(
        Node::Box,
        (fixed(40.0, 20.0), Transform(Affine::translate((1.0, 0.0)))),
    );
    let translucent = ui.create(Node::Box, (fixed(40.0, 20.0), Opacity(0.5)));
    let zero = ui.create(Node::Box, (fixed(40.0, 20.0), z(0)));
    // Paints in the layer of its nearest owning ancestor.
    let nested = ui.create(Node::Box, (fixed(10.0, 10.0),));
    ui.append(root, transformed);
    ui.append(root, translucent);
    ui.append(root, zero);
    ui.append(transformed, nested);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.layers.len(), 4);

    for node in [transformed, translucent, zero] {
        let layer = f.layer_of(node);

        assert_eq!(f.layers[layer].z_index, 0);
        assert_eq!(
            f.entries(layer)[0],
            PaintLayerEntry::Fragment(f.box_of(node))
        );
    }

    assert_eq!(
        f.entries(f.layer_of(transformed)),
        [
            PaintLayerEntry::Fragment(f.box_of(transformed)),
            PaintLayerEntry::Fragment(f.box_of(nested)),
        ]
    );
    assert_eq!(
        f.entries(f.root),
        [
            PaintLayerEntry::Fragment(f.box_of(root)),
            PaintLayerEntry::Layer(f.layer_of(transformed)),
            PaintLayerEntry::Layer(f.layer_of(translucent)),
            PaintLayerEntry::Layer(f.layer_of(zero)),
        ]
    );
}

#[test]
fn only_boxes_start_layers() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline, Opacity(0.5), z(3)));
    ui.append(root, span);
    let text = ui.create(Node::Text, (Text("inline".into()),));
    ui.append(span, text);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.layers.len(), 1);
}

#[test]
fn entries_paint_in_z_index_order_after_their_owner() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (Fill(Paint::Solid(Color::WHITE)),));
    let above = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), z(1), Fill(Paint::Solid(GREEN))),
    );
    let flow = ui.create(Node::Box, (fixed(10.0, 10.0), Fill(Paint::Solid(BLUE))));
    let below = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), z(-1), Fill(Paint::Solid(RED))),
    );
    let also_above = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), z(1), Fill(Paint::Solid(RED))),
    );
    ui.append(root, above);
    ui.append(root, flow);
    ui.append(root, below);
    ui.append(root, also_above);

    let f = Fixture::build(&ui, root);

    // The owner first, then negative layers, flow content, and positive
    // layers in tree order.
    assert_eq!(
        f.entries(f.root),
        [
            PaintLayerEntry::Fragment(f.box_of(root)),
            PaintLayerEntry::Layer(f.layer_of(below)),
            PaintLayerEntry::Fragment(f.box_of(flow)),
            PaintLayerEntry::Layer(f.layer_of(above)),
            PaintLayerEntry::Layer(f.layer_of(also_above)),
        ]
    );
    assert_eq!(f.layers[f.layer_of(below)].z_index, -1);

    assert_eq!(
        commands(&f.draw(&ui)),
        [
            fill(Color::WHITE),
            fill(RED),
            fill(BLUE),
            fill(GREEN),
            fill(RED)
        ]
    );
}

#[test]
fn translucent_layers_paint_into_a_group() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let translucent = ui.create(
        Node::Box,
        (fixed(40.0, 20.0), Opacity(0.5), Fill(Paint::Solid(RED))),
    );
    let content = ui.create(Node::Box, (fixed(10.0, 10.0), Fill(Paint::Solid(GREEN))));
    let transparent = ui.create(
        Node::Box,
        (fixed(40.0, 20.0), Opacity(0.0), Fill(Paint::Solid(BLUE))),
    );
    ui.append(root, translucent);
    ui.append(translucent, content);
    ui.append(root, transparent);

    let f = Fixture::build(&ui, root);

    // Transparent layers are skipped with their contents.
    assert_eq!(
        commands(&f.draw(&ui)),
        ["layer 0.5".into(), fill(RED), fill(GREEN), "pop".into()]
    );
}

#[test]
fn clips_apply_to_descendants_in_other_layers() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let clipper = ui.create(
        Node::Box,
        (
            fixed(50.0, 20.0),
            bx::Overflow(comet_layout::style::geometry::Point::new(
                Overflow::Hidden,
                Overflow::Hidden,
            )),
            bx::Padding(LayoutRect::new_all(StyleUnit::Px(5.0))),
        ),
    );
    let raised = ui.create(
        Node::Box,
        (
            fixed(10.0, 10.0),
            z(1),
            Opacity(0.5),
            Fill(Paint::Solid(RED)),
        ),
    );
    let flow = ui.create(Node::Box, (fixed(10.0, 10.0), Fill(Paint::Solid(BLUE))));
    ui.append(root, clipper);
    ui.append(clipper, raised);
    ui.append(clipper, flow);

    let f = Fixture::build(&ui, root);
    let clip = format!("clip {:?}", Rect::new(0.0, 0.0, 50.0, 20.0));

    // The raised layer paints after the flow content. Its group nests in the
    // clip of its ancestor, which stays pushed until the group is popped.
    assert_eq!(
        commands(&f.draw(&ui)),
        [
            clip,
            fill(BLUE),
            "layer 0.5".into(),
            fill(RED),
            "pop".into(),
            "pop".into(),
        ]
    );
}

fn blur() -> Arc<anyrender::Filter> {
    Arc::new(anyrender::Filter::default())
}

/// Layers pushed to a scene, with their alpha, transform, clip bounds rounded
/// to whole units, and whether they filter their contents and their backdrop.
fn pushed_layers(scene: &Scene) -> Vec<(f32, Affine, Rect, bool, bool)> {
    scene
        .commands
        .iter()
        .filter_map(|command| match command {
            RenderCommand::PushLayer(layer) => Some((
                layer.alpha,
                layer.transform,
                layer.clip.bounding_box().round(),
                layer.filter.is_some(),
                layer.backdrop_filter.is_some(),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn filters_start_layers() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let filtered = ui.create(Node::Box, (fixed(10.0, 10.0), Filter(blur())));
    let backdrop = ui.create(Node::Box, (fixed(10.0, 10.0), BackdropFilter(blur())));
    ui.append(root, filtered);
    ui.append(root, backdrop);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.layers.len(), 3);
    assert_eq!(f.layers[f.layer_of(filtered)].z_index, 0);
    assert_eq!(f.layers[f.layer_of(backdrop)].z_index, 0);
}

#[test]
fn filters_apply_to_a_group_in_the_owner_coordinates() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (bx::Padding(LayoutRect::new_all(StyleUnit::Px(5.0))),),
    );
    let filtered = ui.create(
        Node::Box,
        (fixed(40.0, 20.0), Filter(blur()), Fill(Paint::Solid(RED))),
    );
    let content = ui.create(Node::Box, (fixed(10.0, 10.0), Fill(Paint::Solid(GREEN))));
    ui.append(root, filtered);
    ui.append(filtered, content);

    let f = Fixture::build(&ui, root);
    let scene = f.draw(&ui);

    // Opaque, but grouped for the filter, which applies in local coordinates.
    // The group clip is the viewport, mapped to them.
    assert_eq!(
        pushed_layers(&scene),
        [(
            1.0,
            Affine::translate((5.0, 5.0)),
            Rect::new(-5.0, -5.0, 115.0, 395.0),
            true,
            false
        )]
    );
    assert_eq!(
        commands(&scene),
        ["layer 1".into(), fill(RED), fill(GREEN), "pop".into()]
    );
}

#[test]
fn backdrop_filters_apply_behind_the_owner_border_box() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (Fill(Paint::Solid(Color::WHITE)),));
    let glass = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            BackdropFilter(blur()),
            BorderRadius(LayoutRect::new_all(StyleUnit::Px(4.0))),
            Fill(Paint::Solid(BLUE)),
        ),
    );
    // Overflows the border box, unclipped by the backdrop filter.
    let content = ui.create(Node::Box, (fixed(80.0, 10.0), Fill(Paint::Solid(GREEN))));
    ui.append(root, glass);
    ui.append(glass, content);

    let f = Fixture::build(&ui, root);
    let scene = f.draw(&ui);

    // An empty layer filters the backdrop before the owner paints, without a
    // group for its contents.
    assert_eq!(
        pushed_layers(&scene),
        [(
            1.0,
            Affine::IDENTITY,
            Rect::new(0.0, 0.0, 40.0, 20.0),
            false,
            true
        )]
    );
    assert_eq!(
        commands(&scene),
        [
            fill(Color::WHITE),
            "layer 1".into(),
            "pop".into(),
            fill(BLUE),
            fill(GREEN),
        ]
    );
}

#[test]
fn filters_of_collapsed_owners_paint_nothing() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let collapsed = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            Filter(blur()),
            BackdropFilter(blur()),
            Transform(Affine::scale_non_uniform(0.0, 1.0)),
            Fill(Paint::Solid(RED)),
        ),
    );
    ui.append(root, collapsed);

    let f = Fixture::build(&ui, root);

    assert!(commands(&f.draw(&ui)).is_empty());
}

/// A margin pulling a box up over the previous one.
fn overlap(value: f64) -> bx::Margin {
    let zero = AutoValue::Value(StyleUnit::ZERO);

    bx::Margin(LayoutRect::new_invidiual(
        AutoValue::Value(StyleUnit::Px(-value)),
        zero,
        zero,
        zero,
    ))
}

#[test]
fn hits_the_topmost_fragment_in_paint_order() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let raised = ui.create(Node::Box, (fixed(40.0, 20.0), z(1)));
    // Paints after the raised box in tree order, but below it.
    let flow = ui.create(Node::Box, (fixed(40.0, 20.0), overlap(10.0)));
    ui.append(root, raised);
    ui.append(root, flow);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.hit(5.0, 5.0), Some(f.box_of(raised)));
    assert_eq!(f.hit(5.0, 15.0), Some(f.box_of(raised)));
    assert_eq!(f.hit(5.0, 25.0), Some(f.box_of(flow)));
    assert_eq!(f.hit(100.0, 5.0), Some(f.box_of(root)));
    assert_eq!(f.hit(500.0, 5.0), None);
}

#[test]
fn hits_children_before_parents_and_skips_line_boxes() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let parent = ui.create(Node::Box, (fixed(80.0, 40.0),));
    let child = ui.create(Node::Box, (fixed(20.0, 10.0),));
    ui.append(root, parent);
    ui.append(parent, child);
    let content = ui.create(Node::Text, (Text("text".into()),));
    ui.append(parent, content);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.hit(5.0, 5.0), Some(f.box_of(child)));
    assert_eq!(f.hit(70.0, 35.0), Some(f.box_of(parent)));

    // Text below the child hits its glyph run, not the line holding it.
    let text = f.hit(2.0, 15.0).unwrap();
    assert!(matches!(
        f.fragments.items[text].item,
        FragmentItem::GlyphRun { .. }
    ));
}

#[test]
fn hits_through_transforms() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let moved = ui.create(
        Node::Box,
        (fixed(40.0, 20.0), Transform(Affine::translate((50.0, 0.0)))),
    );
    let inner = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), Transform(Affine::scale(2.0))),
    );
    ui.append(root, moved);
    ui.append(moved, inner);

    let f = Fixture::build(&ui, root);

    // The inner box scales around its centre (5, 5), to (-5, -5)..(15, 15)
    // inside the moved box.
    assert_eq!(f.hit(46.0, 1.0), Some(f.box_of(inner)));
    assert_eq!(f.hit(64.0, 14.0), Some(f.box_of(inner)));
    assert_eq!(f.hit(80.0, 15.0), Some(f.box_of(moved)));
    assert_eq!(f.hit(10.0, 10.0), Some(f.box_of(root)));
}

#[test]
fn clips_and_rounded_corners_limit_hits() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let clipper = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            bx::Overflow(comet_layout::style::geometry::Point::new(
                Overflow::Hidden,
                Overflow::Hidden,
            )),
        ),
    );
    // Overflows the clipper, in a layer of its own.
    let wide = ui.create(Node::Box, (fixed(80.0, 10.0), z(1)));
    let rounded = ui.create(
        Node::Box,
        (
            fixed(40.0, 40.0),
            BorderRadius(LayoutRect::new_all(StyleUnit::Px(10.0))),
        ),
    );
    ui.append(root, clipper);
    ui.append(clipper, wide);
    ui.append(root, rounded);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.hit(10.0, 5.0), Some(f.box_of(wide)));
    assert_eq!(f.hit(60.0, 5.0), Some(f.box_of(root)));

    // The rounded box sits below the clipper, from y 20.
    assert_eq!(f.hit(20.0, 40.0), Some(f.box_of(rounded)));
    assert_eq!(f.hit(1.0, 21.0), Some(f.box_of(root)));
}

#[test]
fn collapsed_fragments_are_not_hit() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let collapsed = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            Transform(Affine::scale_non_uniform(0.0, 1.0)),
        ),
    );
    let content = ui.create(Node::Box, (fixed(40.0, 20.0),));
    ui.append(root, collapsed);
    ui.append(collapsed, content);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.hit(20.0, 10.0), Some(f.box_of(root)));
}

#[test]
fn rebuilding_replaces_the_layers() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let raised = ui.create(Node::Box, (fixed(10.0, 10.0), z(2)));
    ui.append(root, raised);

    let mut f = Fixture::build(&ui, root);
    let old_root = f.root;
    let fragment_root = f.layers[old_root].owner;

    ui.set_props(raised, (fixed(10.0, 10.0), ZIndex(AutoValue::Auto)));
    let new_root = PaintLayerBuilder::new()
        .build(&ui, &f.inputs, &f.fragments, &mut f.layers, fragment_root)
        .unwrap();

    assert_eq!(f.layers.len(), 1);
    assert_eq!(f.layers[new_root].owner, fragment_root);
    assert!(f.layers.get(old_root).is_none());

    // Stale root layers hit nothing instead of panicking.
    assert_eq!(
        PaintHitTester::new().hit(
            &f.layers,
            &f.fragments,
            &f.pre_paint,
            old_root,
            Point::new(1.0, 1.0)
        ),
        None
    );
}

#[test]
fn hit_testers_can_be_reused() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let moved = ui.create(
        Node::Box,
        (fixed(40.0, 20.0), Transform(Affine::translate((50.0, 0.0)))),
    );
    ui.append(root, moved);

    let mut f = Fixture::build(&ui, root);
    let mut tester = PaintHitTester::new();
    let mut hit = |f: &Fixture, x, y| {
        tester.hit(
            &f.layers,
            &f.fragments,
            &f.pre_paint,
            f.root,
            Point::new(x, y),
        )
    };

    // Points mapped by earlier hits do not leak into later ones.
    assert_eq!(hit(&f, 60.0, 10.0), Some(f.box_of(moved)));
    assert_eq!(hit(&f, 10.0, 10.0), Some(f.box_of(root)));
    assert_eq!(hit(&f, 60.0, 10.0), Some(f.box_of(moved)));

    // Nor into hits on rebuilt layers.
    let fragment_root = f.layers[f.root].owner;
    f.root = PaintLayerBuilder::new()
        .build(&ui, &f.inputs, &f.fragments, &mut f.layers, fragment_root)
        .unwrap();
    assert_eq!(hit(&f, 60.0, 10.0), Some(f.box_of(moved)));
}
