use anyrender::{Scene, recording::RenderCommand};
use comet_layout::style::geometry::Rect as LayoutRect;
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{BoxFragmentKey, FragmentRef, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Size, StyleUnit, UnitResolveContext},
    },
};
use comet_paint::hit::{HitTestResult, PaintHitTester};
use comet_paint::style;
use comet_paint::style::BorderRadius;
use comet_paint::style::Paint;
use comet_paint::style::ScrollTop;
use comet_paint::{
    layer::{PaintLayerKey, builder::PaintLayerBuilder, tree::PaintLayerTree},
    painter::CometCompositePainter,
    prepaint::{builder::PrepaintTreeBuilder, tree::PrepaintTree},
    selection::rects::SelectionRects,
    style::{
        BackdropFilter, Background, Filter, MixBlend, Opacity, Transform, TransformOrigin, ZIndex,
    },
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use kurbo::Point;
use kurbo::{Affine, Rect, Shape};
use peniko::{BlendMode, Color, Mix};
use taffy::{Overflow, Position};

const VIEWPORT: Size<f64> = Size::new(120.0, 400.0);
const SCENE_VIEWPORT: Rect = Rect::new(0.0, 0.0, 120.0, 400.0);
const UNITS: UnitResolveContext = LayoutConstraint::new_root(VIEWPORT, 16.0).units;

const RED: Color = Color::from_rgb8(200, 20, 20);
const GREEN: Color = Color::from_rgb8(20, 160, 20);
const BLUE: Color = Color::from_rgb8(20, 40, 180);

/// Layout, pre-paint and layer trees of one UI root.
struct Fixture {
    inputs: LayoutInputTree,
    dirty: DirtyMap,
    fragments: FragmentTree,
    prepaint: PrepaintTree,
    layers: PaintLayerTree,
    /// Root layer of `layers`.
    root: PaintLayerKey,
    builder: PaintLayerBuilder,
}

impl Fixture {
    fn build(ui: &Ui, node: NodeKey) -> Self {
        let mut inputs = LayoutInputTree::new();
        let mut dirty = DirtyMap::new();
        let input_root = LayoutInputTreeBuilder::new()
            .build(
                ui,
                &mut inputs,
                &mut dirty,
                BuildRequest::new(node, 16.0, VIEWPORT),
            )
            .unwrap();
        let mut fragments = FragmentTree::new();
        let root = FragmentTreeBuilder::new()
            .build(
                ui,
                &inputs,
                &mut dirty,
                input_root,
                &mut fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0),
            )
            .unwrap();

        let mut prepaint = PrepaintTree::new();
        PrepaintTreeBuilder::new()
            .build(
                ui,
                &inputs,
                &dirty,
                &fragments,
                &UNITS,
                &mut prepaint,
                root,
                false,
            )
            .unwrap();

        let mut layers = PaintLayerTree::new();
        let mut builder = PaintLayerBuilder::new();
        let root = builder
            .build(
                ui,
                &inputs,
                &dirty,
                &fragments,
                &UNITS,
                &mut layers,
                input_root,
                false,
            )
            .unwrap();
        dirty.clear(&inputs, input_root);

        Self {
            inputs,
            dirty,
            fragments,
            prepaint,
            layers,
            root,
            builder,
        }
    }

    /// Resolve the pre-paint data and layers of a node again, after its own
    /// paint styles changed, and update the layers under it.
    fn update(&mut self, ui: &Ui, node: NodeKey) {
        let fragment_root = self.root_box();

        self.dirty.mark_node(
            &self.inputs,
            node,
            DirtyFlags::NEED_PRE_PAINT | DirtyFlags::NEED_LAYER,
        );
        PrepaintTreeBuilder::new()
            .build(
                ui,
                &self.inputs,
                &self.dirty,
                &self.fragments,
                &UNITS,
                &mut self.prepaint,
                fragment_root,
                false,
            )
            .unwrap();
        let input_root = self.input_root();
        self.root = self
            .builder
            .build(
                ui,
                &self.inputs,
                &self.dirty,
                &self.fragments,
                &UNITS,
                &mut self.layers,
                input_root,
                false,
            )
            .unwrap();
        self.dirty.clear(&self.inputs, input_root);
    }

    /// The layout input item of the root layer's owner.
    fn input_root(&self) -> LayoutInputKey {
        self.fragments.boxes[self.root_box()].source
    }

    /// The box owning the root layer.
    fn root_box(&self) -> BoxFragmentKey {
        self.layers.layers[self.root].owner
    }

    /// A node's box fragment, as a fragment.
    fn box_ref(&self, node: NodeKey) -> FragmentRef {
        FragmentRef::Box(self.box_of(node))
    }

    fn box_of(&self, node: NodeKey) -> BoxFragmentKey {
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
    fn hit(&self, x: f64, y: f64) -> Option<FragmentRef> {
        self.hit_result(x, y).map(|hit| hit.fragment)
    }

    /// The hit at a scene point, with the point local to the fragment hit.
    fn hit_result(&self, x: f64, y: f64) -> Option<HitTestResult> {
        PaintHitTester::new().hit(
            &self.layers,
            &self.fragments,
            &self.prepaint,
            self.root,
            Point::new(x, y),
        )
    }

    /// The hits on the path to the fragment hit at a scene point, from the
    /// root down.
    fn hit_path(&self, x: f64, y: f64) -> Vec<HitTestResult> {
        let mut tester = PaintHitTester::new();
        let mut cursor = tester.cursor(self.root, Point::new(x, y));

        core::iter::from_fn(|| cursor.hit_next(&self.layers, &self.fragments, &self.prepaint))
            .collect()
    }

    fn draw(&self, ui: &Ui) -> Scene {
        let mut scene = Scene::new();
        CometCompositePainter::new().draw(
            ui,
            &self.inputs,
            &self.fragments,
            &self.prepaint,
            &SelectionRects::default(),
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

fn solid(paint: &anyrender::Paint) -> Color {
    match paint {
        anyrender::Paint::Solid(color) => *color,
        _ => panic!("test only uses solid paints"),
    }
}

fn z(value: i32) -> ZIndex {
    ZIndex(AutoValue::Value(value))
}

/// Background colors, layer alphas, clips and pops of a scene, in order.
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
            f.layers.enclosing(&f.fragments, f.box_ref(node)),
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
        f.layers.enclosing(&f.fragments, f.box_ref(nested)),
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
fn pieces_of_inline_elements_start_layers() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline, Opacity(0.5), z(3)));
    ui.append(root, span);
    let text = ui.create(
        Node::Text,
        (Text(
            "one two three four five six seven eight nine ten".into(),
        ),),
    );
    ui.append(span, text);
    // Inline elements are not transformed, so a transform starts no layer.
    let moved = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            Transform(Affine::translate((1.0, 0.0))),
        ),
    );
    ui.append(root, moved);
    let moved_text = ui.create(Node::Text, (Text("moved".into()),));
    ui.append(moved, moved_text);

    let f = Fixture::build(&ui, root);
    let span_item = f.inputs.item_for_node(span).unwrap();
    let pieces: Vec<_> = f.fragments.fragments(&f.inputs, span_item).collect();
    let parts: Vec<_> = f.fragments.inline_boxes(&f.inputs, span_item).collect();
    assert!(pieces.len() > 1);
    assert_eq!(parts.len(), pieces.len());

    // One layer for the whole element, owned by its first inline part box.
    assert_eq!(f.layers.layers.len(), 2);
    let layer = f.layers.layer_of(parts[0]).unwrap();
    assert_eq!(f.children(f.root), [layer]);
    for &part in &parts[1..] {
        assert_eq!(f.layers.layer_of(part), None);
    }

    let item = &f.layers.layers[layer];
    assert_eq!(item.owner, parts[0]);
    assert_eq!(item.z_index, 3);
    assert_eq!(item.opacity, 0.5);

    // It holds every piece, with its glyph runs.
    for piece in pieces {
        assert_eq!(f.layers.enclosing(&f.fragments, piece), Some(layer));
    }
    let text_item = f.inputs.item_for_node(text).unwrap();
    for run in f.fragments.fragments(&f.inputs, text_item) {
        assert_eq!(f.layers.enclosing(&f.fragments, run), Some(layer));
    }
}

#[test]
fn layers_paint_in_z_index_order_around_their_owner() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (Background(Paint::Solid(Color::WHITE)),));
    let above = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), z(1), Background(Paint::Solid(GREEN))),
    );
    let flow = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), Background(Paint::Solid(BLUE))),
    );
    let below = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), z(-1), Background(Paint::Solid(RED))),
    );
    let also_above = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), z(1), Background(Paint::Solid(RED))),
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
        (
            fixed(40.0, 20.0),
            Opacity(0.5),
            Background(Paint::Solid(RED)),
        ),
    );
    let content = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), Background(Paint::Solid(GREEN))),
    );
    let transparent = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            Opacity(0.0),
            Background(Paint::Solid(BLUE)),
        ),
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
            Background(Paint::Solid(RED)),
        ),
    );
    let flow = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), Background(Paint::Solid(BLUE))),
    );
    ui.append(root, clipper);
    ui.append(clipper, raised);
    ui.append(clipper, flow);

    let f = Fixture::build(&ui, root);
    let clip = format!("clip {:?}", Rect::new(0.0, 0.0, 50.0, 20.0));

    // The clipper hides its overflow, so it starts a layer: the raised layer
    // is its child, painting after the flow content inside its clip. Its
    // group nests in that clip.
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

fn blur() -> Vec<anyrender::filters::FilterEffect> {
    vec![anyrender::filters::FilterEffect::blur(2.0)]
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
        (
            fixed(40.0, 20.0),
            Filter(blur()),
            Background(Paint::Solid(RED)),
        ),
    );
    let content = ui.create(
        Node::Box,
        (fixed(10.0, 10.0), Background(Paint::Solid(GREEN))),
    );
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
    let root = ui.create(Node::Box, (Background(Paint::Solid(Color::WHITE)),));
    let glass = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            BackdropFilter(blur()),
            BorderRadius(LayoutRect::new_all(StyleUnit::Px(4.0))),
            Background(Paint::Solid(BLUE)),
        ),
    );
    // Overflows the border box, unclipped by the backdrop filter.
    let content = ui.create(
        Node::Box,
        (fixed(80.0, 10.0), Background(Paint::Solid(GREEN))),
    );
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
            Background(Paint::Solid(RED)),
        ),
    );
    ui.append(root, collapsed);

    let f = Fixture::build(&ui, root);

    assert!(commands(&f.draw(&ui)).is_empty());
}

#[test]
fn mix_blend_modes_start_layers_and_blend_their_group() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let blended = ui.create(
        Node::Box,
        (
            fixed(40.0, 20.0),
            MixBlend(BlendMode::from(Mix::Multiply)),
            Background(Paint::Solid(RED)),
        ),
    );
    ui.append(root, blended);

    let mut f = Fixture::build(&ui, root);
    let layer = f.layer_of(blended);

    assert_eq!(f.layers.layers.len(), 2);
    assert_eq!(f.layers.layers[layer].z_index, 0);
    assert_eq!(
        f.layers.layers[layer].blend_mode,
        Some(BlendMode::from(Mix::Multiply))
    );

    let blends = |scene: &Scene| -> Vec<BlendMode> {
        scene
            .commands
            .iter()
            .filter_map(|command| match command {
                RenderCommand::PushLayer(layer) => Some(layer.blend),
                _ => None,
            })
            .collect()
    };
    // Opaque and unfiltered, but grouped for the blend mode.
    assert_eq!(blends(&f.draw(&ui)), [BlendMode::from(Mix::Multiply)]);

    // Changing the blend mode updates the layer in place.
    ui.set_props(blended, (MixBlend(BlendMode::from(Mix::Screen)),));
    f.update(&ui, blended);

    assert_eq!(f.layer_of(blended), layer);
    assert_eq!(
        f.layers.layers[layer].blend_mode,
        Some(BlendMode::from(Mix::Screen))
    );
    assert_eq!(blends(&f.draw(&ui)), [BlendMode::from(Mix::Screen)]);
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

    assert_eq!(f.hit(5.0, 5.0), Some(f.box_ref(raised)));
    assert_eq!(f.hit(5.0, 15.0), Some(f.box_ref(raised)));
    assert_eq!(f.hit(5.0, 25.0), Some(f.box_ref(flow)));
    assert_eq!(f.hit(100.0, 5.0), Some(f.box_ref(root)));
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

    assert_eq!(f.hit(5.0, 5.0), Some(f.box_ref(child)));
    assert_eq!(f.hit(70.0, 35.0), Some(f.box_ref(parent)));

    // Text below the child hits its glyph run, not the line holding it.
    let FragmentRef::Inline { owner, index } = f.hit(2.0, 15.0).unwrap() else {
        panic!("expected an inline item");
    };
    // The block splits the parent's inline content, so a run box holds it.
    assert_eq!(f.fragments.boxes.parent(owner), Some(f.box_of(parent)));
    assert!(f.fragments.inline_layout(owner).unwrap().items[index as usize].is_run());
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
    assert_eq!(f.hit(46.0, 1.0), Some(f.box_ref(inner)));
    assert_eq!(f.hit(64.0, 14.0), Some(f.box_ref(inner)));
    assert_eq!(f.hit(80.0, 15.0), Some(f.box_ref(moved)));
    assert_eq!(f.hit(10.0, 10.0), Some(f.box_ref(root)));
}

#[test]
fn hit_cursor_walks_from_the_root_down_to_the_fragment_hit() {
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
    let result = |key, x, y| HitTestResult {
        fragment: f.box_ref(key),
        position: Point::new(x, y),
    };

    // Each fragment with the point in its own coordinates; the inner box
    // scales by 2 around its centre (5, 5), inside the moved box at (50, 0).
    assert_eq!(
        f.hit_path(46.0, 1.0),
        [
            result(root, 46.0, 1.0),
            result(moved, -4.0, 1.0),
            result(inner, 0.5, 3.0),
        ]
    );
    assert_eq!(
        f.hit_path(46.0, 1.0).last().copied(),
        f.hit_result(46.0, 1.0),
        "hit is the end of the path"
    );
    assert_eq!(f.hit_path(80.0, 15.0).len(), 2, "down to the moved box");
    assert!(f.hit_path(500.0, 5.0).is_empty(), "nothing under the point");
}

#[test]
fn hits_hold_the_point_local_to_the_fragment_hit() {
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
    let result = |key, x, y| {
        Some(HitTestResult {
            fragment: f.box_ref(key),
            position: Point::new(x, y),
        })
    };

    // The inner box scales by 2 around its centre (5, 5), inside the moved
    // box at (50, 0).
    assert_eq!(f.hit_result(46.0, 1.0), result(inner, 0.5, 3.0));
    assert_eq!(f.hit_result(64.0, 14.0), result(inner, 9.5, 9.5));
    assert_eq!(f.hit_result(80.0, 15.0), result(moved, 30.0, 15.0));
    assert_eq!(f.hit_result(10.0, 10.0), result(root, 10.0, 10.0));
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

    assert_eq!(f.hit(10.0, 5.0), Some(f.box_ref(wide)));
    assert_eq!(f.hit(60.0, 5.0), Some(f.box_ref(root)));

    // The rounded box sits below the clipper, from y 20.
    assert_eq!(f.hit(20.0, 40.0), Some(f.box_ref(rounded)));
    assert_eq!(f.hit(1.0, 21.0), Some(f.box_ref(root)));
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

    assert_eq!(f.hit(20.0, 10.0), Some(f.box_ref(root)));
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
    let input_root = f.input_root();
    f.root = f
        .builder
        .build(
            &ui,
            &f.inputs,
            &f.dirty,
            &f.fragments,
            &UNITS,
            &mut f.layers,
            input_root,
            false,
        )
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
    let fragment_root = f.root_box();
    let right_item = f.inputs.item_for_node(right).unwrap();
    let items: Vec<_> = f.inputs.items.descendants(right_item).collect();
    for item in items {
        f.dirty.mark(&f.inputs, item, DirtyFlags::NEED_LAYER);
    }
    PrepaintTreeBuilder::new()
        .build(
            &ui,
            &f.inputs,
            &f.dirty,
            &f.fragments,
            &UNITS,
            &mut f.prepaint,
            fragment_root,
            false,
        )
        .unwrap();
    let input_root = f.input_root();
    f.root = f
        .builder
        .build(
            &ui,
            &f.inputs,
            &f.dirty,
            &f.fragments,
            &UNITS,
            &mut f.layers,
            input_root,
            false,
        )
        .unwrap();
    f.dirty.clear(&f.inputs, input_root);

    let mut full = PaintLayerTree::new();
    let full_root = PaintLayerBuilder::new()
        .build(
            &ui,
            &f.inputs,
            &f.dirty,
            &f.fragments,
            &UNITS,
            &mut full,
            input_root,
            false,
        )
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
            &f.prepaint,
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
        tester
            .hit(
                &f.layers,
                &f.fragments,
                &f.prepaint,
                f.root,
                Point::new(x, y),
            )
            .map(|hit| hit.fragment)
    };

    // Points mapped by earlier hits do not leak into later ones.
    assert_eq!(hit(&f, 60.0, 10.0), Some(f.box_ref(moved)));
    assert_eq!(hit(&f, 10.0, 10.0), Some(f.box_ref(root)));
    assert_eq!(hit(&f, 60.0, 10.0), Some(f.box_ref(moved)));

    // Nor into hits on rebuilt layers.
    let input_root = f.input_root();
    f.root = PaintLayerBuilder::new()
        .build(
            &ui,
            &f.inputs,
            &f.dirty,
            &f.fragments,
            &UNITS,
            &mut f.layers,
            input_root,
            false,
        )
        .unwrap();
    assert_eq!(hit(&f, 60.0, 10.0), Some(f.box_ref(moved)));
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
    assert_eq!(f.hit(20.0, 5.0), Some(f.box_ref(lowest)));
    // The later negative layer is above the earlier one.
    assert_eq!(f.hit(20.0, 15.0), Some(f.box_ref(low)));
    // Contents are above negative layers, and positive layers above both.
    assert_eq!(f.hit(20.0, 25.0), Some(f.box_ref(flow)));
    assert_eq!(f.hit(5.0, 15.0), Some(f.box_ref(raised)));
    assert_eq!(f.hit(50.0, 5.0), Some(f.box_ref(root)));
}

/// Every stage of one UI root, kept to update after UI changes.
#[derive(Default)]
struct Pipeline {
    input_builder: LayoutInputTreeBuilder,
    fragment_builder: FragmentTreeBuilder,
    resolver: PrepaintTreeBuilder,
    builder: PaintLayerBuilder,
    inputs: LayoutInputTree,
    dirty: DirtyMap,
    fragments: FragmentTree,
    prepaint: PrepaintTree,
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
                &mut self.dirty,
                BuildRequest::new(node, 16.0, VIEWPORT),
            )
            .unwrap();
        let root = self
            .fragment_builder
            .build(
                ui,
                &self.inputs,
                &mut self.dirty,
                input_root,
                &mut self.fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0),
            )
            .unwrap();
        self.resolver
            .build(
                ui,
                &self.inputs,
                &self.dirty,
                &self.fragments,
                &UNITS,
                &mut self.prepaint,
                root,
                false,
            )
            .unwrap();

        let layer = self
            .builder
            .build(
                ui,
                &self.inputs,
                &self.dirty,
                &self.fragments,
                &UNITS,
                &mut self.layers,
                input_root,
                false,
            )
            .unwrap();
        self.dirty.clear(&self.inputs, input_root);

        layer
    }

    fn box_of(&self, node: NodeKey) -> Option<BoxFragmentKey> {
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
    fn full_shape(&self, ui: &Ui, root: PaintLayerKey) -> String {
        let input_root = self.fragments.boxes[self.layers.layers[root].owner].source;
        let mut full = PaintLayerTree::new();
        let full_root = PaintLayerBuilder::new()
            .build(
                ui,
                &self.inputs,
                &self.dirty,
                &self.fragments,
                &UNITS,
                &mut full,
                input_root,
                false,
            )
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
    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));

    // A new box starting a layer, inserted before the others.
    let added = ui.create(Node::Box, (fixed(60.0, 10.0), z(1)));
    ui.prepend(root, added);
    p.dirty
        .mark_node(&p.inputs, root, DirtyFlags::NEED_CHILDREN);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));
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
    p.dirty
        .mark_node(&p.inputs, root, DirtyFlags::NEED_CHILDREN);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));
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
    p.dirty.mark_node(&p.inputs, group, DirtyFlags::NEED_LAYER);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));
    assert_eq!(p.layer_of(group), None);
    assert_eq!(p.children(root_layer), [nested_layer, sibling_layer]);

    // Starting one again takes the nested layer back.
    ui.set_props(group, (z(2),));
    p.dirty.mark_node(&p.inputs, group, DirtyFlags::NEED_LAYER);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));
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
    p.dirty.mark_node(&p.inputs, first, DirtyFlags::NEED_LAYER);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));
    assert_eq!(p.children(root_layer), [second_layer, first_layer]);
    assert_eq!(p.layers.layers[first_layer].z_index, 3);
}

#[test]
fn reordered_siblings_reorder_layers_with_equal_z_indices() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let boxes: Vec<_> = (0..3)
        .map(|_| {
            let node = ui.create(Node::Box, (fixed(60.0, 20.0), z(1)));
            ui.append(root, node);
            node
        })
        .collect();

    let mut p = Pipeline::default();
    let root_layer = p.run(&ui, root);
    let layers: Vec<_> = boxes
        .iter()
        .map(|&node| p.layer_of(node).unwrap())
        .collect();
    assert_eq!(p.children(root_layer), layers);

    // Equal z indices paint in tree order, which follows the move.
    ui.prepend(root, boxes[2]);
    p.dirty
        .mark_node(&p.inputs, root, DirtyFlags::NEED_CHILDREN);
    assert_eq!(p.run(&ui, root), root_layer);

    // Every owner keeps its layer, in the new order.
    assert_eq!(p.layer_of(boxes[2]), Some(layers[2]));
    assert_eq!(
        p.children(root_layer),
        vec![layers[2], layers[0], layers[1]]
    );
    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));
}

#[test]
fn reordered_boxes_reorder_the_layers_inside_them() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let plain = ui.create(Node::Box, (fixed(60.0, 20.0),));
    let wrapper = ui.create(Node::Box, ());
    let inner = ui.create(Node::Box, (fixed(60.0, 20.0), z(1)));
    let sibling = ui.create(Node::Box, (fixed(60.0, 20.0), z(1)));
    ui.append(root, plain);
    ui.append(root, wrapper);
    ui.append(wrapper, inner);
    ui.append(root, sibling);

    let mut p = Pipeline::default();
    let root_layer = p.run(&ui, root);
    let inner_layer = p.layer_of(inner).unwrap();
    let sibling_layer = p.layer_of(sibling).unwrap();
    assert_eq!(p.children(root_layer), vec![inner_layer, sibling_layer]);

    // The wrapper starts no layer, but moving it after the sibling moves the
    // layer inside it too.
    ui.append(root, wrapper);
    p.dirty
        .mark_node(&p.inputs, root, DirtyFlags::NEED_CHILDREN);
    assert_eq!(p.run(&ui, root), root_layer);

    let by_node = [sibling, inner].map(|node| p.layer_of(node).unwrap());
    assert_eq!(p.children(root_layer), by_node);
    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));
}

#[test]
fn reordered_atomic_inlines_reorder_their_layers() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let atomic = |ui: &mut Ui| {
        ui.create(
            Node::Box,
            (
                bx::DisplayOuter::Inline,
                bx::DisplayInner::FlowRoot,
                fixed(20.0, 20.0),
                z(1),
            ),
        )
    };
    let first = atomic(&mut ui);
    let second = atomic(&mut ui);
    let text = ui.create(Node::Text, (Text("text".into()),));
    ui.append(root, first);
    ui.append(root, text);
    ui.append(root, second);

    let mut p = Pipeline::default();
    let root_layer = p.run(&ui, root);
    let first_layer = p.layer_of(first).unwrap();
    let second_layer = p.layer_of(second).unwrap();
    assert_eq!(p.children(root_layer), vec![first_layer, second_layer]);

    ui.prepend(root, second);
    p.dirty
        .mark_node(&p.inputs, root, DirtyFlags::NEED_CHILDREN);
    assert_eq!(p.run(&ui, root), root_layer);

    let by_node = [second, first].map(|node| p.layer_of(node).unwrap());
    assert_eq!(p.children(root_layer), by_node);
    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));
}

fn top_left(top: f64, left: f64) -> bx::Inset {
    bx::Inset(LayoutRect::new_invidiual(
        AutoValue::Value(StyleUnit::Px(top)),
        AutoValue::Auto,
        AutoValue::Auto,
        AutoValue::Value(StyleUnit::Px(left)),
    ))
}

#[test]
fn hits_descendants_overflowing_their_parents() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let parent = ui.create(Node::Box, (fixed(20.0, 20.0),));
    // Moved out of the parent, to (60, 30)..(80, 40).
    let moved = ui.create(Node::Box, (fixed(20.0, 10.0), top_left(30.0, 60.0)));
    ui.append(root, parent);
    ui.append(parent, moved);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.hit(65.0, 35.0), Some(f.box_ref(moved)));
    // Its far corner, on the edge of the parent's overflow.
    assert_eq!(f.hit(79.9, 39.9), Some(f.box_ref(moved)));
    assert_eq!(f.hit(10.0, 10.0), Some(f.box_ref(parent)));
    assert_eq!(f.hit(40.0, 10.0), Some(f.box_ref(root)));
    // Below the 20px tall root, beside the moved box.
    assert_eq!(f.hit(40.0, 35.0), None);
}

#[test]
fn hits_hoisted_boxes_placed_outside_their_parents() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    // Static, so the root positions the absolute box, above the wrapper.
    let wrapper = ui.create(
        Node::Box,
        (
            bx::Position(Position::Static),
            fixed(40.0, 20.0),
            bx::Margin(LayoutRect::new_invidiual(
                AutoValue::Value(StyleUnit::Px(100.0)),
                AutoValue::Value(StyleUnit::ZERO),
                AutoValue::Value(StyleUnit::ZERO),
                AutoValue::Value(StyleUnit::ZERO),
            )),
        ),
    );
    let inner = ui.create(
        Node::Box,
        (bx::Position(Position::Static), fixed(40.0, 20.0)),
    );
    let abs = ui.create(
        Node::Box,
        (
            bx::Position(Position::Absolute),
            top_left(5.0, 7.0),
            fixed(10.0, 10.0),
        ),
    );
    ui.append(root, wrapper);
    ui.append(wrapper, inner);
    ui.append(inner, abs);

    let f = Fixture::build(&ui, root);

    assert_eq!(f.hit(10.0, 10.0), Some(f.box_ref(abs)));
    assert_eq!(f.hit(10.0, 110.0), Some(f.box_ref(inner)));
}

#[test]
fn hits_content_of_atomic_inlines_below_their_line() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let paragraph = ui.create(Node::Box, (fixed(200.0, 20.0),));
    let inline_block = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            bx::DisplayInner::FlowRoot,
            fixed(20.0, 10.0),
        ),
    );
    // Overflows the atomic inline, far below the line and the paragraph.
    let tall = ui.create(Node::Box, (fixed(10.0, 100.0),));
    ui.append(root, paragraph);
    ui.append(paragraph, inline_block);
    ui.append(inline_block, tall);

    let f = Fixture::build(&ui, root);
    let tall_box = f.box_of(tall);

    // Somewhere in its lower half, in scene coordinates.
    let mut origin = Point::ZERO;
    let mut key = Some(tall_box);
    while let Some(k) = key {
        let location = f.fragments.boxes[k].layout.geometry.location;
        origin += kurbo::Vec2::new(location.x, location.y);
        key = f.fragments.boxes.parent(k);
    }

    assert_eq!(
        f.hit(origin.x + 5.0, origin.y + 80.0),
        Some(FragmentRef::Box(tall_box))
    );
}

/// A root holding a 50x30 scroll container at the origin, scrolled down by
/// 10, with `extra` props, and three 40x20 rows inside, with `row_extra`
/// props on the second. Returns the UI, the root, the container and the
/// rows.
fn scrolled_rows(
    extra: impl comet_tree::style::StyleProps,
    row_extra: impl comet_tree::style::StyleProps,
) -> (Ui, NodeKey, NodeKey, [NodeKey; 3]) {
    let mut ui = Ui::new();
    // Tall enough to be hit past the container.
    let root = ui.create(Node::Box, (fixed(120.0, 100.0),));
    let scroller = ui.create(
        Node::Box,
        (
            fixed(50.0, 30.0),
            bx::Overflow(comet_layout::style::geometry::Point::new(
                Overflow::Hidden,
                Overflow::Hidden,
            )),
            ScrollTop(10.0),
        ),
    );
    ui.set_props(scroller, extra);
    ui.append(root, scroller);

    let rows = [(); 3].map(|_| {
        let row = ui.create(Node::Box, (fixed(40.0, 20.0),));
        ui.append(scroller, row);
        row
    });
    ui.set_props(rows[1], row_extra);

    (ui, root, scroller, rows)
}

/// Rows scrolled up by 10 sit at -10..10, 10..30 and 30..50 in the scene,
/// the last one hidden past the container's clip at 30.
fn assert_scrolled_hits(f: &Fixture, root: NodeKey, scroller: NodeKey, rows: [NodeKey; 3]) {
    let result = |key, x, y| {
        Some(HitTestResult {
            fragment: f.box_ref(key),
            position: Point::new(x, y),
        })
    };

    assert_eq!(f.hit_result(5.0, 5.0), result(rows[0], 5.0, 15.0));
    assert_eq!(f.hit_result(5.0, 15.0), result(rows[1], 5.0, 5.0));
    // The container is hit where no row is; it does not scroll itself.
    assert_eq!(f.hit_result(45.0, 5.0), result(scroller, 45.0, 5.0));
    // The third row is clipped away.
    assert_eq!(f.hit_result(5.0, 35.0), result(root, 5.0, 35.0));
}

#[test]
fn hits_follow_the_scroll_of_their_container() {
    let (ui, root, scroller, rows) = scrolled_rows((), ());
    let f = Fixture::build(&ui, root);

    assert_scrolled_hits(&f, root, scroller, rows);
}

#[test]
fn hits_scroll_with_containers_owning_layers() {
    let (ui, root, scroller, rows) = scrolled_rows((Opacity(0.5),), ());
    let f = Fixture::build(&ui, root);

    assert_scrolled_hits(&f, root, scroller, rows);
}

#[test]
fn hits_in_child_layers_scroll_with_their_container() {
    let (ui, root, scroller, rows) = scrolled_rows((), (Transform(Affine::IDENTITY),));
    let f = Fixture::build(&ui, root);

    assert_scrolled_hits(&f, root, scroller, rows);
}

#[test]
fn hits_in_layers_tested_alone_scroll_with_their_ancestors() {
    let (ui, root, _, rows) = scrolled_rows((), (Transform(Affine::IDENTITY),));
    let f = Fixture::build(&ui, root);

    assert_eq!(
        PaintHitTester::new().hit(
            &f.layers,
            &f.fragments,
            &f.prepaint,
            f.layer_of(rows[1]),
            Point::new(5.0, 15.0),
        ),
        Some(HitTestResult {
            fragment: f.box_ref(rows[1]),
            position: Point::new(5.0, 5.0),
        })
    );
}

impl Pipeline {
    /// The inline part boxes of an inline element, in layout order.
    fn parts(&self, node: NodeKey) -> Vec<BoxFragmentKey> {
        let item = self.inputs.item_for_node(node).unwrap();

        self.fragments.inline_boxes(&self.inputs, item).collect()
    }

    /// The layer of an inline element, owned by its first inline part box.
    fn element_layer(&self, node: NodeKey) -> Option<PaintLayerKey> {
        self.layers.layer_of(*self.parts(node).first()?)
    }
}

#[test]
fn layers_of_inline_elements_follow_their_pieces() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (fixed(120.0, 200.0),));
    let lead = ui.create(Node::Text, (Text("lead ".into()),));
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline, Opacity(0.5)));
    let text = ui.create(
        Node::Text,
        (Text(
            "one two three four five six seven eight nine ten".into(),
        ),),
    );
    ui.append(root, lead);
    ui.append(root, span);
    ui.append(span, text);

    let mut p = Pipeline::default();
    let root_layer = p.run(&ui, root);
    let parts = p.parts(span);
    assert!(parts.len() > 1);
    assert_eq!(p.layers.layers.len(), 2);

    let check = |p: &Pipeline, ui: &Ui| {
        assert_eq!(shape(&p.layers, root_layer), p.full_shape(ui, root_layer));
        assert_eq!(p.layers.layers.len(), 2);
        let layer = p.element_layer(span).unwrap();
        assert_eq!(p.layers.layers[layer].owner, p.parts(span)[0]);
        assert_eq!(p.children(root_layer), [layer]);
    };
    check(&p, &ui);

    // Laid out again on the same lines, with new inline part boxes.
    ui.set_props(root, (fixed(120.0, 300.0),));
    p.dirty
        .mark_node(&p.inputs, root, DirtyFlags::NEED_BOX_LAYOUT);
    assert_eq!(p.run(&ui, root), root_layer);
    assert_eq!(p.parts(span).len(), parts.len());
    check(&p, &ui);

    // Narrower, the span splits into more pieces, still in one layer.
    ui.set_props(root, (fixed(60.0, 300.0),));
    p.dirty
        .mark_node(&p.inputs, root, DirtyFlags::NEED_BOX_LAYOUT);
    assert_eq!(p.run(&ui, root), root_layer);
    assert!(p.parts(span).len() > parts.len());
    check(&p, &ui);

    // And wider again.
    ui.set_props(root, (fixed(120.0, 300.0),));
    p.dirty
        .mark_node(&p.inputs, root, DirtyFlags::NEED_BOX_LAYOUT);
    assert_eq!(p.run(&ui, root), root_layer);
    check(&p, &ui);
}

#[test]
fn layers_of_inline_elements_follow_their_order() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline, z(1)));
    let text = ui.create(Node::Text, (Text("span".into()),));
    ui.append(root, span);
    ui.append(span, text);

    let mut p = Pipeline::default();
    let root_layer = p.run(&ui, root);
    assert!(p.element_layer(span).is_some());

    // Another span before it, with a higher z index.
    let before = ui.create(Node::Box, (bx::DisplayOuter::Inline, z(2)));
    let before_text = ui.create(Node::Text, (Text("before ".into()),));
    ui.prepend(root, before);
    ui.append(before, before_text);
    p.dirty
        .mark_node(&p.inputs, root, DirtyFlags::NEED_CHILDREN);
    assert_eq!(p.run(&ui, root), root_layer);

    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));
    assert_eq!(p.layers.layers.len(), 3);
    assert_eq!(
        p.children(root_layer),
        [
            p.element_layer(span).unwrap(),
            p.element_layer(before).unwrap()
        ]
    );
}

#[test]
fn layers_of_inline_elements_cover_pieces_split_by_blocks() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline, Opacity(0.5)));
    let lead = ui.create(Node::Text, (Text("lead".into()),));
    let block = ui.create(Node::Box, (fixed(20.0, 10.0),));
    let tail = ui.create(Node::Text, (Text("tail".into()),));
    ui.append(root, span);
    ui.append(span, lead);
    ui.append(span, block);
    ui.append(span, tail);

    let mut p = Pipeline::default();
    let root_layer = p.run(&ui, root);

    // The block splits the span into pieces in two runs, linked.
    let parts = p.parts(span);
    assert_eq!(parts.len(), 2);
    assert_ne!(
        p.fragments.boxes.parent(parts[0]),
        p.fragments.boxes.parent(parts[1])
    );

    // One layer covers both.
    assert_eq!(p.layers.layers.len(), 2);
    let layer = p.element_layer(span).unwrap();
    for node in [lead, tail] {
        let item = p.inputs.item_for_node(node).unwrap();
        for run in p.fragments.fragments(&p.inputs, item) {
            assert_eq!(p.layers.enclosing(&p.fragments, run), Some(layer));
        }
    }
    assert_eq!(shape(&p.layers, root_layer), p.full_shape(&ui, root_layer));
}

/// Fills, glyph runs and layers of a scene, with the depth of the layers
/// pushed around each, in order.
fn painted(scene: &Scene) -> Vec<(String, usize)> {
    let mut depth = 0;
    let mut painted = Vec::new();

    for command in &scene.commands {
        match command {
            RenderCommand::Fill(fill) => painted.push((fill_name(&fill.brush), depth)),
            RenderCommand::GlyphRun(run) => {
                painted.push((format!("text {:?}", solid(&run.brush).to_rgba8()), depth));
            }
            RenderCommand::PushLayer(_) | RenderCommand::PushClipLayer(_) => depth += 1,
            RenderCommand::PopLayer => depth -= 1,
            _ => {}
        }
    }

    painted
}

fn fill_name(paint: &anyrender::Paint) -> String {
    fill(solid(paint))
}

fn text(color: Color) -> String {
    format!("text {:?}", color.to_rgba8())
}

#[test]
fn inline_element_layers_paint_every_piece_with_its_items() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (fixed(80.0, 200.0),));
    let lead = ui.create(Node::Text, (Text("lead ".into()),));
    let span = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            Opacity(0.5),
            Background(Paint::Solid(GREEN)),
            style::Color(AutoValue::Value(Paint::Solid(RED))),
        ),
    );
    let wrapped = ui.create(Node::Text, (Text("one two three four five six".into()),));
    // An atomic inline inside the span, painting in its layer.
    let atomic = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            bx::DisplayInner::FlowRoot,
            fixed(10.0, 10.0),
            Background(Paint::Solid(BLUE)),
        ),
    );
    ui.append(root, lead);
    ui.append(root, span);
    ui.append(span, wrapped);
    ui.append(span, atomic);

    let f = Fixture::build(&ui, root);
    let span_item = f.inputs.item_for_node(span).unwrap();
    let parts = f.fragments.inline_boxes(&f.inputs, span_item).count();
    assert!(parts > 1);

    // One group holds the pieces' backgrounds first, then their items.
    let painted = painted(&f.draw(&ui));
    let groups: Vec<_> = painted.iter().filter(|(_, depth)| *depth > 0).collect();
    let black = text(Color::BLACK);

    assert_eq!(painted.iter().filter(|(name, _)| *name == black).count(), 1);
    assert!(painted.iter().any(|entry| *entry == (black.clone(), 0)));
    assert_eq!(groups.len(), painted.len() - 1);
    assert!(groups[..parts].iter().all(|(name, _)| *name == fill(GREEN)));
    assert!(groups[parts..].iter().all(|(name, _)| *name != fill(GREEN)));
    assert!(groups.iter().any(|(name, _)| *name == text(RED)));
    assert!(groups.iter().any(|(name, _)| *name == fill(BLUE)));
}

#[test]
fn inline_element_layers_paint_above_the_content_around_them() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            z(1),
            Background(Paint::Solid(GREEN)),
            style::Color(AutoValue::Value(Paint::Solid(RED))),
        ),
    );
    let inside = ui.create(Node::Text, (Text("inside".into()),));
    let tail = ui.create(Node::Text, (Text(" tail".into()),));
    ui.append(root, span);
    ui.append(span, inside);
    ui.append(root, tail);

    let f = Fixture::build(&ui, root);

    // The span's piece and its glyphs are skipped by the root's contents, and
    // painted after them by its layer.
    let names: Vec<_> = painted(&f.draw(&ui))
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, [text(Color::BLACK), fill(GREEN), text(RED)]);
}

#[test]
fn hit_paths_pass_through_the_pieces_of_inline_elements() {
    for layered in [false, true] {
        let mut ui = Ui::new();
        let root = ui.create(Node::Box, ());
        let lead = ui.create(Node::Text, (Text("lead ".into()),));
        let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
        if layered {
            ui.set_props(span, (Opacity(0.5),));
        }
        let content = ui.create(Node::Text, (Text("in".into()),));
        let atomic = ui.create(
            Node::Box,
            (
                bx::DisplayOuter::Inline,
                bx::DisplayInner::FlowRoot,
                fixed(20.0, 20.0),
            ),
        );
        ui.append(root, lead);
        ui.append(root, span);
        ui.append(span, content);
        ui.append(span, atomic);

        let f = Fixture::build(&ui, root);
        let item = |node| f.inputs.item_for_node(node).unwrap();
        let piece = f.fragments.fragments(&f.inputs, item(span)).next().unwrap();
        let run = f
            .fragments
            .fragments(&f.inputs, item(content))
            .next()
            .unwrap();
        let center = |fragment| {
            let geometry = f.fragments.geometry(fragment).unwrap();
            Point::new(
                geometry.location.x + geometry.size.width / 2.0,
                geometry.location.y + geometry.size.height / 2.0,
            )
        };
        let path = |point: Point| -> Vec<_> {
            f.hit_path(point.x, point.y)
                .into_iter()
                .map(|hit| hit.fragment)
                .collect()
        };
        let root_box = f.box_ref(root);

        // A glyph run inside the span, with the point local to it.
        let point = center(run);
        assert_eq!(path(point), [root_box, piece, run], "layered: {layered}");
        let location = f.fragments.geometry(run).unwrap().location;
        assert_eq!(
            f.hit_result(point.x, point.y).unwrap().position,
            point - kurbo::Vec2::new(location.x, location.y)
        );

        // An atomic inline inside the span is a box held by its piece.
        let atomic_box = f.box_ref(atomic);
        assert_eq!(
            path(center(atomic_box)),
            [root_box, piece, atomic_box],
            "layered: {layered}"
        );
    }
}

/// Build the layers of `f` again, after the marked changes, with `units`.
fn rebuild(f: &mut Fixture, ui: &Ui, units: &UnitResolveContext, viewport_changed: bool) {
    let input_root = f.input_root();
    f.root = f
        .builder
        .build(
            ui,
            &f.inputs,
            &f.dirty,
            &f.fragments,
            units,
            &mut f.layers,
            input_root,
            viewport_changed,
        )
        .unwrap();
    f.dirty.clear(&f.inputs, input_root);
}

#[test]
fn layers_resolve_transforms_around_their_origin() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (bx::Padding(LayoutRect::new_all(StyleUnit::Px(5.0))),),
    );
    let scaled = ui.create(
        Node::Box,
        (fixed(20.0, 10.0), Transform(Affine::scale(2.0))),
    );
    let plain = ui.create(Node::Box, (fixed(20.0, 10.0),));
    ui.append(root, scaled);
    ui.append(root, plain);

    let mut f = Fixture::build(&ui, root);
    let around_centre =
        Affine::translate((10.0, 5.0)) * Affine::scale(2.0) * Affine::translate((-10.0, -5.0));

    let item = &f.layers.layers[f.layer_of(scaled)];
    assert_eq!(item.transform, around_centre);
    assert_eq!(item.inverse_transform, Some(around_centre.inverse()));
    // Boxes are placed at their location, after their own transform.
    assert_eq!(
        f.layers.transform(&f.fragments, f.box_of(scaled)),
        Some(Affine::translate((5.0, 5.0)) * around_centre)
    );
    assert_eq!(
        f.layers.transform(&f.fragments, f.box_of(plain)),
        Some(Affine::translate((5.0, 15.0)))
    );
    assert_eq!(
        f.layers.inverse_transform(&f.fragments, f.box_of(plain)),
        Some(Affine::translate((-5.0, -15.0)))
    );

    // Changing the transform only needs the layer resolved again.
    ui.set_props(scaled, (Transform(Affine::translate((1.0, 2.0))),));
    f.dirty.mark_node(&f.inputs, scaled, DirtyFlags::NEED_LAYER);
    rebuild(&mut f, &ui, &UNITS, false);

    assert_eq!(
        f.layers.transform(&f.fragments, f.box_of(scaled)),
        Some(Affine::translate((6.0, 7.0)))
    );

    // A collapsing transform has no inverse.
    ui.set_props(scaled, (Transform(Affine::scale_non_uniform(0.0, 1.0)),));
    f.dirty.mark_node(&f.inputs, scaled, DirtyFlags::NEED_LAYER);
    rebuild(&mut f, &ui, &UNITS, false);

    assert_eq!(
        f.layers.inverse_transform(&f.fragments, f.box_of(scaled)),
        None
    );
}

#[test]
fn viewport_changes_resolve_viewport_relative_transform_origins() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let rotated = ui.create(
        Node::Box,
        (
            fixed(20.0, 10.0),
            Transform(Affine::scale(2.0)),
            TransformOrigin(comet_layout::style::geometry::Point::new(
                StyleUnit::Vw(0.1),
                StyleUnit::Px(0.0),
            )),
        ),
    );
    ui.append(root, rotated);

    let mut f = Fixture::build(&ui, root);
    let around =
        |x: f64| Affine::translate((x, 0.0)) * Affine::scale(2.0) * Affine::translate((-x, 0.0));
    // 10% of the 120 wide viewport.
    assert_eq!(f.layers.layers[f.layer_of(rotated)].transform, around(12.0));

    let wide = LayoutConstraint::new_root(Size::new(240.0, 400.0), 16.0).units;

    // Unmarked layers keep their data until told the viewport changed.
    rebuild(&mut f, &ui, &wide, false);
    assert_eq!(f.layers.layers[f.layer_of(rotated)].transform, around(12.0));

    rebuild(&mut f, &ui, &wide, true);
    assert_eq!(f.layers.layers[f.layer_of(rotated)].transform, around(24.0));
}
