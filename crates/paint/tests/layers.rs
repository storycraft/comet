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
    layer::{PaintLayerKey, builder::PaintLayerBuilder, tree::PaintLayerTree},
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
    builder: PaintLayerBuilder,
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
        let mut builder = PaintLayerBuilder::new();
        let root = builder
            .build(&fragments, &pre_paint, &mut layers, root)
            .unwrap();
        pre_paint.clear_layer_flags();

        Self {
            inputs,
            fragments,
            pre_paint,
            layers,
            root,
            builder,
        }
    }

    /// Resolve the pre-paint data of a node's box fragment again, after its
    /// own paint styles changed, and update the layers under it.
    fn update(&mut self, ui: &Ui, node: NodeKey) {
        let key = self.box_of(node);
        let fragment_root = self.layers.layers[self.root].owner;

        self.pre_paint.mark(&self.fragments, key);
        PrePaintTreeResolver::new()
            .resolve(
                ui,
                &self.inputs,
                &self.fragments,
                &UNITS,
                &mut self.pre_paint,
                fragment_root,
            )
            .unwrap();
        self.pre_paint.clear_flags();
        self.root = self
            .builder
            .build(
                &self.fragments,
                &self.pre_paint,
                &mut self.layers,
                fragment_root,
            )
            .unwrap();
        self.pre_paint.clear_layer_flags();
    }

    fn box_of(&self, node: NodeKey) -> FragmentKey {
        let item = self.inputs.item_for_node(node).unwrap();

        self.fragments.box_fragment(item).unwrap()
    }

    /// The layer owned by a node's box fragment.
    fn layer_of(&self, node: NodeKey) -> PaintLayerKey {
        self.layers.layer_of(self.box_of(node)).unwrap()
    }

    /// The child layers of a layer, in paint order.
    fn children(&self, layer: PaintLayerKey) -> Vec<PaintLayerKey> {
        self.layers.layers.children(layer).collect()
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
            1.0,
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

    assert_eq!(f.layers.layers.len(), 1);
    assert_eq!(f.layers.layers[f.root].owner, f.box_of(root));
    assert_eq!(f.layers.layers.parent(f.root), None);
    assert!(f.children(f.root).is_empty());

    for node in [outer, inner] {
        assert_eq!(f.layers.layer_of(f.box_of(node)), None);
        assert_eq!(
            f.layers.enclosing(&f.fragments, f.box_of(node)),
            Some(f.root)
        );
    }
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

    assert_eq!(f.layers.layers.len(), 4);

    for node in [transformed, translucent, zero] {
        let layer = f.layer_of(node);

        assert_eq!(f.layers.layers[layer].owner, f.box_of(node));
        assert_eq!(f.layers.layers[layer].z_index, 0);
        assert_eq!(f.layers.layers.parent(layer), Some(f.root));
    }

    assert_eq!(f.layers.layer_of(f.box_of(nested)), None);
    assert_eq!(
        f.layers.enclosing(&f.fragments, f.box_of(nested)),
        Some(f.layer_of(transformed))
    );
    assert_eq!(
        f.children(f.root),
        [
            f.layer_of(transformed),
            f.layer_of(translucent),
            f.layer_of(zero)
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

    assert_eq!(f.layers.layers.len(), 1);
}

#[test]
fn layers_paint_in_z_index_order_around_their_owner() {
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

    // Child layers sort by z index, keeping tree order among equals. The
    // owner paints first, then negative layers, flow content, and the other
    // layers.
    assert_eq!(
        f.children(f.root),
        [f.layer_of(below), f.layer_of(above), f.layer_of(also_above)]
    );
    assert_eq!(f.layers.layers[f.layer_of(below)].z_index, -1);

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

    // The raised layer paints after the flow content, inside the clip of its
    // ancestor pushed again around it. Its group nests in that clip.
    assert_eq!(
        commands(&f.draw(&ui)),
        [
            clip.clone(),
            fill(BLUE),
            "pop".into(),
            clip,
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

    assert_eq!(f.layers.layers.len(), 3);
    assert_eq!(f.layers.layers[f.layer_of(filtered)].z_index, 0);
    assert_eq!(f.layers.layers[f.layer_of(backdrop)].z_index, 0);
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
fn building_again_keeps_layer_keys() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let raised = ui.create(Node::Box, (fixed(10.0, 10.0), z(2)));
    let moved = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), Transform(Affine::translate((5.0, 0.0)))),
    );
    ui.append(root, raised);
    ui.append(root, moved);

    let mut f = Fixture::build(&ui, root);
    let root_layer = f.root;
    let moved_layer = f.layer_of(moved);

    // The raised box stops starting a layer.
    ui.set_props(raised, (ZIndex(AutoValue::Auto),));
    f.update(&ui, raised);

    assert_eq!(f.root, root_layer);
    assert_eq!(f.layers.layers.len(), 2);
    assert_eq!(f.layers.layer_of(f.box_of(raised)), None);
    assert_eq!(f.layer_of(moved), moved_layer);
    assert_eq!(f.children(f.root), [moved_layer]);

    // Building from the root keeps them too.
    let fragment_root = f.layers.layers[f.root].owner;
    f.root = f
        .builder
        .build(&f.fragments, &f.pre_paint, &mut f.layers, fragment_root)
        .unwrap();

    assert_eq!(f.root, root_layer);
    assert_eq!(f.layer_of(moved), moved_layer);
}

#[test]
fn layers_of_stopped_owners_move_to_their_parent() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let group = ui.create(Node::Box, (fixed(40.0, 40.0), Opacity(0.5)));
    let raised = ui.create(Node::Box, (fixed(10.0, 10.0), z(1)));
    let below = ui.create(Node::Box, (fixed(10.0, 10.0), z(-1)));
    ui.append(root, group);
    ui.append(group, raised);
    ui.append(group, below);

    let mut f = Fixture::build(&ui, root);
    let raised_layer = f.layer_of(raised);
    let below_layer = f.layer_of(below);

    assert_eq!(
        f.layers.layers.parent(raised_layer),
        Some(f.layer_of(group))
    );

    ui.remove_prop::<Opacity>(group);
    f.update(&ui, group);

    // Placed in the root layer by z index, with the same keys.
    assert_eq!(f.layers.layer_of(f.box_of(group)), None);
    assert_eq!(f.children(f.root), [below_layer, raised_layer]);

    // And back again.
    ui.set_props(group, (Opacity(0.5),));
    f.update(&ui, group);

    let group_layer = f.layer_of(group);
    assert_eq!(f.children(f.root), [group_layer]);
    assert_eq!(f.children(group_layer), [below_layer, raised_layer]);
}

/// The layers under `layer`, with their owners and z indices, nested.
fn shape(layers: &PaintLayerTree, layer: PaintLayerKey) -> String {
    let item = &layers.layers[layer];
    let children: Vec<_> = layers
        .layers
        .children(layer)
        .map(|child| shape(layers, child))
        .collect();

    format!("{:?}@{}[{}]", item.owner, item.z_index, children.join(", "))
}

#[test]
fn updates_match_full_builds() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let left = ui.create(Node::Box, (fixed(60.0, 60.0), z(1)));
    let right = ui.create(Node::Box, (fixed(60.0, 60.0),));
    let deep = ui.create(Node::Box, (fixed(20.0, 20.0),));
    let kept = ui.create(Node::Box, (fixed(20.0, 20.0), z(3)));
    let inner = ui.create(Node::Box, (fixed(10.0, 10.0), Opacity(0.5)));
    ui.append(root, left);
    ui.append(root, right);
    ui.append(left, kept);
    ui.append(right, deep);
    ui.append(deep, inner);

    let mut f = Fixture::build(&ui, root);
    let kept_layer = f.layer_of(kept);

    // Deep in the right box: a new layer above an existing one, which also
    // changes z index.
    ui.set_props(deep, (z(-2),));
    ui.set_props(inner, (z(4),));
    let key = f.box_of(right);
    let fragment_root = f.layers.layers[f.root].owner;
    for fragment in f.fragments.items.descendants(key) {
        f.pre_paint.mark(&f.fragments, fragment);
    }
    PrePaintTreeResolver::new()
        .resolve(
            &ui,
            &f.inputs,
            &f.fragments,
            &UNITS,
            &mut f.pre_paint,
            fragment_root,
        )
        .unwrap();
    f.pre_paint.clear_flags();
    f.root = f
        .builder
        .build(&f.fragments, &f.pre_paint, &mut f.layers, fragment_root)
        .unwrap();
    f.pre_paint.clear_layer_flags();

    let fragment_root = f.layers.layers[f.root].owner;
    let mut full = PaintLayerTree::new();
    let full_root = PaintLayerBuilder::new()
        .build(&f.fragments, &f.pre_paint, &mut full, fragment_root)
        .unwrap();

    assert_eq!(shape(&f.layers, f.root), shape(&full, full_root));
    assert_eq!(f.layers.layers.len(), 5);
    // Layers outside the updated subtree are left as they are.
    assert_eq!(f.layer_of(kept), kept_layer);
    assert_eq!(f.layers.layers[f.layer_of(deep)].z_index, -2);
    assert_eq!(
        f.layers.layers.parent(f.layer_of(inner)),
        Some(f.layer_of(deep))
    );
}

#[test]
fn stale_root_layers_hit_nothing() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());

    let mut f = Fixture::build(&ui, root);
    let old_root = f.root;
    f.layers.clear();

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
    let fragment_root = f.layers.layers[f.root].owner;
    f.root = PaintLayerBuilder::new()
        .build(&f.fragments, &f.pre_paint, &mut f.layers, fragment_root)
        .unwrap();
    assert_eq!(hit(&f, 60.0, 10.0), Some(f.box_of(moved)));
}

#[test]
fn layer_styles_start_layers_whatever_their_value() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let identity = ui.create(Node::Box, (fixed(10.0, 10.0), Transform(Affine::IDENTITY)));
    let opaque = ui.create(Node::Box, (fixed(10.0, 10.0), Opacity(1.0)));
    let auto = ui.create(Node::Box, (fixed(10.0, 10.0), ZIndex(AutoValue::Auto)));
    ui.append(root, identity);
    ui.append(root, opaque);
    ui.append(root, auto);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.layers.layers[f.layer_of(identity)].z_index, 0);
    assert_eq!(f.layers.layers[f.layer_of(opaque)].z_index, 0);
    // An auto z index sets no value.
    assert_eq!(f.layers.layer_of(f.box_of(auto)), None);
}

#[test]
fn hits_negative_layers_below_contents_from_the_last() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let lowest = ui.create(Node::Box, (fixed(40.0, 20.0), z(-2)));
    let low = ui.create(Node::Box, (fixed(40.0, 20.0), z(-1), overlap(10.0)));
    let flow = ui.create(Node::Box, (fixed(40.0, 20.0), overlap(10.0)));
    let raised = ui.create(Node::Box, (fixed(10.0, 20.0), z(1), overlap(30.0)));
    ui.append(root, lowest);
    ui.append(root, low);
    ui.append(root, flow);
    ui.append(root, raised);

    let f = Fixture::build(&ui, root);

    // Boxes from y 0, 10 and 20, and the narrow raised box from y 10.
    assert_eq!(f.hit(20.0, 5.0), Some(f.box_of(lowest)));
    // The later negative layer is above the earlier one.
    assert_eq!(f.hit(20.0, 15.0), Some(f.box_of(low)));
    // Contents are above negative layers, and positive layers above both.
    assert_eq!(f.hit(20.0, 25.0), Some(f.box_of(flow)));
    assert_eq!(f.hit(5.0, 15.0), Some(f.box_of(raised)));
    assert_eq!(f.hit(50.0, 5.0), Some(f.box_of(root)));
}

/// Every stage of one UI root, kept to update after UI changes.
#[derive(Default)]
struct Pipeline {
    input_builder: LayoutInputTreeBuilder,
    fragment_builder: FragmentTreeBuilder,
    resolver: PrePaintTreeResolver,
    builder: PaintLayerBuilder,
    inputs: LayoutInputTree,
    fragments: FragmentTree,
    pre_paint: PrePaintTree,
    layers: PaintLayerTree,
}

impl Pipeline {
    /// Build or update every stage, returning the root layer.
    fn run(&mut self, ui: &Ui, node: NodeKey) -> PaintLayerKey {
        let input_root = self
            .input_builder
            .build(
                ui,
                &mut self.inputs,
                BuildRequest::new(node, 16.0, VIEWPORT),
            )
            .unwrap();
        let root = self
            .fragment_builder
            .build(
                ui,
                &self.inputs,
                input_root,
                &mut self.fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0),
            )
            .unwrap();
        self.inputs.clear_flags(input_root);
        self.resolver
            .resolve(
                ui,
                &self.inputs,
                &self.fragments,
                &UNITS,
                &mut self.pre_paint,
                root,
            )
            .unwrap();
        self.pre_paint.clear_flags();

        let layer = self
            .builder
            .build(&self.fragments, &self.pre_paint, &mut self.layers, root)
            .unwrap();
        self.pre_paint.clear_layer_flags();

        layer
    }

    fn box_of(&self, node: NodeKey) -> Option<FragmentKey> {
        self.fragments
            .box_fragment(self.inputs.item_for_node(node)?)
    }

    fn layer_of(&self, node: NodeKey) -> Option<PaintLayerKey> {
        self.layers.layer_of(self.box_of(node)?)
    }

    fn children(&self, layer: PaintLayerKey) -> Vec<PaintLayerKey> {
        self.layers.layers.children(layer).collect()
    }

    /// The layers built from scratch for the current trees.
    fn full_shape(&self, root: PaintLayerKey) -> String {
        let fragment_root = self.layers.layers[root].owner;
        let mut full = PaintLayerTree::new();
        let full_root = PaintLayerBuilder::new()
            .build(&self.fragments, &self.pre_paint, &mut full, fragment_root)
            .unwrap();

        shape(&full, full_root)
    }
}

#[test]
fn layers_follow_layout_changes_from_the_root() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let kept = ui.create(Node::Box, (fixed(60.0, 20.0), z(1)));
    let group = ui.create(Node::Box, (fixed(60.0, 60.0), z(2)));
    let nested = ui.create(Node::Box, (fixed(20.0, 20.0), z(-1)));
    ui.append(root, kept);
    ui.append(root, group);
    ui.append(group, nested);

    let mut p = Pipeline::default();
    let root_layer = p.run(&ui, root);
    let kept_layer = p.layer_of(kept).unwrap();
    let nested_layer = p.layer_of(nested).unwrap();
    assert_eq!(p.layers.layers.len(), 4);

    // Nothing changed: the layers stay as they are.
    assert_eq!(p.run(&ui, root), root_layer);
    assert_eq!(shape(&p.layers, root_layer), p.full_shape(root_layer));

    // A new box starting a layer, inserted before the others.
    let added = ui.create(Node::Box, (fixed(60.0, 10.0), z(1)));
    ui.prepend(root, added);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(root_layer));
    assert_eq!(p.layers.layers.len(), 5);
    assert_eq!(p.layer_of(kept), Some(kept_layer));
    assert_eq!(p.layer_of(nested), Some(nested_layer));
    assert_eq!(
        p.children(root_layer)[..2],
        [p.layer_of(added).unwrap(), kept_layer]
    );

    // A deleted owner takes its layer along, and the nested layer of its
    // deleted descendant.
    ui.delete(group);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(root_layer));
    assert_eq!(p.layers.layers.len(), 3);
    assert_eq!(p.layer_of(kept), Some(kept_layer));
    assert!(p.layers.layers.get(nested_layer).is_none());
}

#[test]
fn layers_of_owners_losing_their_stacking_move_up_from_the_root() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let group = ui.create(Node::Box, (fixed(60.0, 60.0), z(2)));
    let nested = ui.create(Node::Box, (fixed(20.0, 20.0), z(-1)));
    let sibling = ui.create(Node::Box, (fixed(60.0, 20.0), z(1)));
    ui.append(root, group);
    ui.append(group, nested);
    ui.append(root, sibling);

    let mut p = Pipeline::default();
    let root_layer = p.run(&ui, root);
    let nested_layer = p.layer_of(nested).unwrap();
    let sibling_layer = p.layer_of(sibling).unwrap();

    // The group stops starting a layer, so its nested layer paints in the
    // root layer, below the sibling.
    ui.set_props(group, (ZIndex(AutoValue::Auto),));
    let group_fragment = p.box_of(group).unwrap();
    p.pre_paint.mark(&p.fragments, group_fragment);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(root_layer));
    assert_eq!(p.layer_of(group), None);
    assert_eq!(p.children(root_layer), [nested_layer, sibling_layer]);

    // Starting one again takes the nested layer back.
    ui.set_props(group, (z(2),));
    p.pre_paint.mark(&p.fragments, group_fragment);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(root_layer));
    let group_layer = p.layer_of(group).unwrap();
    assert_eq!(p.children(root_layer), [sibling_layer, group_layer]);
    assert_eq!(p.children(group_layer), [nested_layer]);
}

#[test]
fn changed_z_indices_reorder_layers_from_the_root() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let first = ui.create(Node::Box, (fixed(60.0, 20.0), z(1)));
    let second = ui.create(Node::Box, (fixed(60.0, 20.0), z(2)));
    ui.append(root, first);
    ui.append(root, second);

    let mut p = Pipeline::default();
    let root_layer = p.run(&ui, root);
    let first_layer = p.layer_of(first).unwrap();
    let second_layer = p.layer_of(second).unwrap();
    assert_eq!(p.children(root_layer), [first_layer, second_layer]);

    ui.set_props(first, (z(3),));
    let fragment = p.box_of(first).unwrap();
    p.pre_paint.mark(&p.fragments, fragment);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(root_layer));
    assert_eq!(p.children(root_layer), [second_layer, first_layer]);
    assert_eq!(p.layers.layers[first_layer].z_index, 3);
}

#[test]
fn layers_ignore_stacking_changes_nobody_marked() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let raised = ui.create(Node::Box, (fixed(60.0, 20.0), z(1)));
    ui.append(root, raised);

    let mut p = Pipeline::default();
    let root_layer = p.run(&ui, root);
    let raised_layer = p.layer_of(raised).unwrap();

    // Not marked: layout alone does not see paint styles, so the layer
    // stays until the fragment is marked.
    ui.set_props(raised, (ZIndex(AutoValue::Auto),));
    assert_eq!(p.run(&ui, root), root_layer);
    assert_eq!(p.layer_of(raised), Some(raised_layer));

    // Clearing the pre-paint tree builds every layer again.
    p.pre_paint.clear();
    assert_eq!(p.run(&ui, root), root_layer);
    assert_eq!(p.layer_of(raised), None);
    assert_eq!(shape(&p.layers, root_layer), p.full_shape(root_layer));
}
