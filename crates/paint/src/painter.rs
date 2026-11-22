//! Painting of fragments into a scene, in the order of their paint layers.

use std::sync::Arc;

use anyrender::AntiAliasing;
use anyrender::Paint;
use anyrender::PaintScene;
use bytemuck::TransparentWrapper;
use comet_layout::{
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
};
use comet_tree::{node::NodeKey, ui::Ui};
use kurbo::{Affine, Insets, Point, Rect, RoundedRect, RoundedRectRadii, Stroke, Vec2};
use parley::PositionedLayoutItem;
use peniko::BlendMode;
use slotmap::SecondaryMap;

use crate::{
    layer::{PaintLayerKey, tree::PaintLayerTree},
    prepaint::{FragmentData, LayoutInputData, tree::PrepaintTree},
    style::{BorderFill, Fill, OutlineFill, TextColor},
};

/// Paints final fragments in the order of their [`PaintLayerTree`], with the
/// [`PrepaintTree`] resolved for them. Keep it to reuse its buffers across
/// paints.
#[derive(Default)]
pub struct CometPainter {
    /// Fragments to paint in the layer being painted.
    pending: Vec<Visit>,
    /// Ancestors of a child layer's owner, up to the owner of its parent
    /// layer, innermost first.
    chain: Vec<FragmentKey>,
    /// Text color of each item looked up while painting, cleared on every
    /// paint.
    text_colors: SecondaryMap<LayoutInputKey, Paint>,
    /// Items looked up for a text color, innermost first.
    items: Vec<LayoutInputKey>,
}

/// A step of painting the fragments of a layer.
enum Visit {
    /// Paint a fragment and queue its children.
    Fragment {
        key: FragmentKey,
        /// Line containing the fragment, which owns the inline layout of its
        /// glyph runs.
        inline_owner: Option<FragmentKey>,
        /// Maps the local coordinates of the fragment's parent to scene
        /// coordinates.
        parent: Transforms,
        /// Scene bounds of the clips the fragment paints inside of.
        clip: Option<Rect>,
    },
    /// Pop the clip layer of a fragment's contents.
    PopClip,
}

/// A transform from local fragment coordinates to scene coordinates, and its
/// inverse.
#[derive(Clone, Copy)]
struct Transforms {
    transform: Affine,
    /// `None` when the transform collapses an axis and has no inverse.
    inverse: Option<Affine>,
}

impl Transforms {
    const IDENTITY: Self = Self {
        transform: Affine::IDENTITY,
        inverse: Some(Affine::IDENTITY),
    };

    /// The transforms of a fragment whose parent maps to the scene with
    /// `self`.
    fn then(self, own: &FragmentData) -> Self {
        Self {
            transform: self.transform * own.transform,
            inverse: own
                .inverse_transform
                .zip(self.inverse)
                .map(|(own, parent)| own * parent),
        }
    }

    /// The transforms mapping the local coordinates of `key` to the scene,
    /// multiplied from the root down, or `None` if a fragment on the way is
    /// not resolved.
    fn of(tree: &FragmentTree, prepaint: &PrepaintTree, key: FragmentKey) -> Option<Self> {
        let parent = match tree.items.parent(key) {
            Some(parent) => Self::of(tree, prepaint, parent)?,
            None => Self::IDENTITY,
        };

        Some(parent.then(prepaint.fragment(key)?))
    }
}

/// What painting reads from.
#[derive(Clone, Copy)]
struct Sources<'a> {
    ui: &'a Ui,
    inputs: &'a LayoutInputTree,
    tree: &'a FragmentTree,
    prepaint: &'a PrepaintTree,
    layers: &'a PaintLayerTree,
    /// The scene's viewport, in physical pixels.
    viewport: Rect,
    /// Physical pixels per logical layout unit.
    scale_factor: f64,
}

impl CometPainter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Paint the fragments under the owner of `root`, a layer of `layers`.
    /// `inputs` is the input tree the fragments were built from, and
    /// `prepaint` and `layers` must be resolved and built for them. Paints
    /// nothing if `root` is not in `layers`.
    ///
    /// Each layer paints its owner fragment, then its child layers with a
    /// negative z index, then its other fragments in tree order, then its
    /// other child layers, with the opacity and filters of its owner's item.
    /// A layer with an opacity below 1, a [`Filter`](crate::style::Filter)
    /// or a [`MixBlend`](crate::style::MixBlend) paints into a group, blended
    /// with that opacity and blend mode and filtered in the owner's local
    /// coordinates. A transparent layer is skipped, and so is the group of a
    /// layer whose owner collapses. A
    /// [`BackdropFilter`](crate::style::BackdropFilter) filters what is
    /// painted behind the owner's border box, rounded with its border radius,
    /// before the layer paints.
    ///
    /// Each fragment paints with its resolved data, such as its transform and
    /// border radius, and the text color of its source item. Fragment
    /// transforms multiply down the fragment tree, starting with the
    /// transforms of the root's ancestors.
    ///
    /// A box fragment clipping its overflow clips its descendants to its
    /// border box, rounded with its border radius, including those painting
    /// in other layers. Clips of the root's ancestors do not apply.
    ///
    /// Layout and pre-paint data are in logical units. The scene is in
    /// physical pixels: everything paints scaled by `scale_factor`, the
    /// physical pixels per logical unit of the display, and text is shaped at
    /// its logical size but drawn at its physical font size. Paints nothing
    /// unless `scale_factor` is positive and finite.
    ///
    /// Fragments painting entirely outside `viewport`, in physical pixels, or
    /// outside their clip are skipped. Fragments touching their edges are
    /// painted. Their descendants are skipped with them only when the
    /// fragment's [overflow](comet_layout::fragment::FragmentBox::overflow)
    /// is outside too, since children may paint outside their parents. Child layers are visited on their own,
    /// so they paint wherever their transforms place them.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        tree: &FragmentTree,
        prepaint: &PrepaintTree,
        layers: &PaintLayerTree,
        root: PaintLayerKey,
        viewport: Rect,
        scale_factor: f64,
        scene: &mut impl PaintScene,
    ) {
        if !(scale_factor > 0.0 && scale_factor.is_finite()) {
            return;
        }

        let Some(item) = layers.layers.get(root) else {
            return;
        };
        let parent = match tree.items.parent(item.owner) {
            Some(parent) => Transforms::of(tree, prepaint, parent),
            None => Some(Transforms::IDENTITY),
        };
        let Some(parent) = parent else {
            return;
        };

        // Logical layout coordinates scale to physical scene pixels at the
        // root, so every transform below includes the display scale.
        let parent = Transforms {
            transform: Affine::scale(scale_factor) * parent.transform,
            inverse: parent
                .inverse
                .map(|inverse| inverse * Affine::scale(scale_factor.recip())),
        };

        let sources = Sources {
            ui,
            inputs,
            tree,
            prepaint,
            layers,
            viewport,
            scale_factor,
        };

        self.pending.clear();
        self.text_colors.clear();
        self.draw_layer(sources, root, parent, None, scene);
    }

    /// Paint a layer, after its owner's backdrop filter and inside a group
    /// with its owner's opacity, filter and blend mode. `parent` maps the local
    /// coordinates of the owner's parent to the scene, and `clip` bounds the
    /// clips the owner paints inside of, already pushed.
    fn draw_layer(
        &mut self,
        sources: Sources<'_>,
        key: PaintLayerKey,
        parent: Transforms,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) {
        let Some(layer) = sources.layers.layers.get(key) else {
            return;
        };
        let owner = layer.owner;
        let (Some(fragment), Some(own)) = (
            sources.tree.items.get(owner),
            sources.prepaint.fragment(owner),
        ) else {
            return;
        };

        let opacity = layer.opacity;

        if opacity <= 0.0 {
            return;
        }

        let filter = layer.filter.clone();
        let backdrop_filter = layer.backdrop_filter.clone();
        let transforms = parent.then(own);

        if let Some(backdrop_filter) = backdrop_filter {
            draw_backdrop(
                sources,
                owner,
                fragment,
                transforms,
                clip,
                backdrop_filter,
                scene,
            );
        }

        let blend_mode = layer.blend_mode;
        let group = opacity < 1.0 || filter.is_some() || blend_mode.is_some();

        if group {
            // A collapsed owner paints nothing, and its contents with it.
            let Some(inverse) = transforms.inverse else {
                return;
            };

            scene.push_layer(
                peniko::Fill::NonZero,
                blend_mode.unwrap_or_default(),
                opacity,
                transforms.transform,
                &inverse.transform_rect_bbox(sources.viewport),
                filter,
                None,
            );
        }

        // The owner paints outside its own clip, and its descendants inside
        // it, child layers included.
        self.draw_fragment(sources, owner, transforms, None, clip, scene);
        let (clip, clipped) = push_clip(sources, owner, fragment, transforms, clip, scene);

        // The first child layer with a non-negative z index, where painting
        // child layers resumes after the contents.
        let mut non_negative = None;

        for child in sources.layers.layers.children(key) {
            if sources.layers.layers[child].z_index >= 0 {
                non_negative = Some(child);
                break;
            }

            self.draw_child(sources, child, owner, transforms, clip, scene);
        }

        if subtree_visible(sources, fragment, transforms, clip) {
            self.draw_contents(sources, owner, transforms, clip, scene);
        }

        for child in sources.layers.layers.cursor(non_negative) {
            self.draw_child(sources, child, owner, transforms, clip, scene);
        }

        if clipped {
            scene.pop_layer();
        }

        if group {
            scene.pop_layer();
        }
    }

    /// Paint a child layer of the layer owned by `owner`, inside the clips of
    /// the fragments between their owners. `transforms` maps the local
    /// coordinates of `owner` to the scene, and `clip` bounds the clips its
    /// contents paint inside of.
    fn draw_child(
        &mut self,
        sources: Sources<'_>,
        child: PaintLayerKey,
        owner: FragmentKey,
        transforms: Transforms,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) {
        let Some(child_owner) = sources.layers.layers.get(child).map(|item| item.owner) else {
            return;
        };

        // Fragments between the owners, innermost first.
        self.chain.clear();
        let mut next = sources.tree.items.parent(child_owner);

        while let Some(fragment) = next
            && fragment != owner
        {
            self.chain.push(fragment);
            next = sources.tree.items.parent(fragment);
        }

        // An outdated layer not under the owner paints nowhere.
        if next.is_none() {
            return;
        }

        let mut transforms = transforms;
        let mut clip = clip;
        let mut pushed = 0;

        for &key in self.chain.iter().rev() {
            let (Some(fragment), Some(own)) =
                (sources.tree.items.get(key), sources.prepaint.fragment(key))
            else {
                continue;
            };

            transforms = transforms.then(own);

            let clipped;
            (clip, clipped) = push_clip(sources, key, fragment, transforms, clip, scene);
            pushed += usize::from(clipped);
        }

        self.draw_layer(sources, child, transforms, clip, scene);

        for _ in 0..pushed {
            scene.pop_layer();
        }
    }

    /// Paint the descendants of a layer's owner in tree order, skipping the
    /// owners of child layers and their descendants.
    fn draw_contents(
        &mut self,
        sources: Sources<'_>,
        owner: FragmentKey,
        transforms: Transforms,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) {
        let base = self.pending.len();

        self.pending
            .extend(
                sources
                    .tree
                    .items
                    .children(owner)
                    .rev()
                    .map(|child| Visit::Fragment {
                        key: child,
                        inline_owner: None,
                        parent: transforms,
                        clip,
                    }),
            );

        while self.pending.len() > base {
            let Some(visit) = self.pending.pop() else {
                break;
            };

            let (key, mut inline_owner, parent, clip) = match visit {
                Visit::Fragment {
                    key,
                    inline_owner,
                    parent,
                    clip,
                } => (key, inline_owner, parent, clip),
                Visit::PopClip => {
                    scene.pop_layer();
                    continue;
                }
            };

            // Child layers paint on their own.
            if sources.layers.layer_of(key).is_some() {
                continue;
            }

            let (Some(fragment), Some(own)) =
                (sources.tree.items.get(key), sources.prepaint.fragment(key))
            else {
                continue;
            };

            if matches!(fragment.item, FragmentItem::Line { .. }) {
                inline_owner = Some(key);
            }

            let transforms = parent.then(own);
            self.draw_fragment(sources, key, transforms, inline_owner, clip, scene);

            // Nothing below paints in view: skip the whole subtree.
            if !subtree_visible(sources, fragment, transforms, clip) {
                continue;
            }

            // A clipping fragment clips its descendants, not itself.
            let (child_clip, clipped) = push_clip(sources, key, fragment, transforms, clip, scene);

            if clipped {
                self.pending.push(Visit::PopClip);
            }

            self.pending
                .extend(
                    sources
                        .tree
                        .items
                        .children(key)
                        .rev()
                        .map(|child| Visit::Fragment {
                            key: child,
                            inline_owner,
                            parent: transforms,
                            clip: child_clip,
                        }),
                );
        }
    }

    /// The text color of an item: the [`TextColor`] of the nearest item
    /// among it and its ancestors setting one, or the default. Inherited
    /// along the input item tree, not the fragment tree, so a block inside an
    /// inline span inherits from the span.
    ///
    /// Looked up from the UI and kept until the next paint, along with the
    /// items on the way, so each item is looked up at most once per paint.
    fn text_color(&mut self, sources: Sources<'_>, key: LayoutInputKey) -> &Paint {
        if !self.text_colors.contains_key(key) {
            let items = &sources.inputs.items;
            let mut next = Some(key);

            self.items.clear();

            let color = loop {
                let Some(key) = next else {
                    break TextColor::default().0;
                };

                if let Some(color) = self.text_colors.get(key) {
                    break color.clone();
                }

                self.items.push(key);

                let own = items
                    .get(key)
                    .and_then(|item| sources.ui.props(item.node))
                    .and_then(|props| props.get_cloned());

                if let Some(TextColor(color)) = own {
                    break color;
                }

                next = items.parent(key);
            };

            for &key in &self.items {
                self.text_colors.insert(key, color.clone());
            }
        }

        &self.text_colors[key]
    }

    /// Paint one fragment itself, without its descendants, inside the clips
    /// bounded by `clip`.
    fn draw_fragment(
        &mut self,
        sources: Sources<'_>,
        key: FragmentKey,
        transforms: Transforms,
        inline_owner: Option<FragmentKey>,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) {
        let Sources {
            ui,
            inputs,
            tree,
            prepaint,
            viewport,
            ..
        } = sources;
        let Some(fragment) = tree.items.get(key) else {
            return;
        };
        let data = prepaint.get(fragment.source);
        let paint_data = prepaint.fragment(key);
        let rect = local_rect(fragment);
        let bounds = box_bounds(fragment, data, rect);
        let transform = transforms.transform;
        let visible = |bounds: Rect| {
            let bounds = transform.transform_rect_bbox(bounds);

            bounds.overlaps(viewport) && clip.is_none_or(|clip| bounds.overlaps(clip))
        };

        match fragment.item {
            FragmentItem::Box { .. } => {
                // A standalone text root also owns a layout box. Its paint
                // belongs to its glyph fragments, not to both representations.
                if inputs
                    .items
                    .get(fragment.source)
                    .is_some_and(|item| item.kind != LayoutInputKind::Text)
                    && visible(bounds)
                {
                    draw_box(
                        ui, inputs, fragment, data, paint_data, rect, transform, scene,
                    );
                }
            }
            FragmentItem::Inline { .. } => {
                if visible(bounds) {
                    draw_box(
                        ui, inputs, fragment, data, paint_data, rect, transform, scene,
                    );
                }
            }
            // Anonymous blocks only position their lines.
            FragmentItem::AnonymousBlock { .. } => {}
            FragmentItem::Line { .. } => {}
            FragmentItem::GlyphRun { line, item } => {
                if visible(bounds) {
                    draw_box(
                        ui, inputs, fragment, data, paint_data, rect, transform, scene,
                    );
                }

                if let Some(owner) = inline_owner {
                    let text_color = self.text_color(sources, fragment.source);

                    draw_glyph_run(
                        tree,
                        fragment,
                        owner,
                        (line, item),
                        rect,
                        transform,
                        sources.scale_factor,
                        text_color,
                        &visible,
                        scene,
                    );
                }
            }
        }
    }
}

/// Whether a fragment's descendants may paint inside the viewport and
/// `clip`: whether its layout overflow overlaps them. `transforms` maps its local
/// coordinates to the scene.
fn subtree_visible(
    sources: Sources<'_>,
    fragment: &Fragment,
    transforms: Transforms,
    clip: Option<Rect>,
) -> bool {
    let bounds = transforms
        .transform
        .transform_rect_bbox(fragment.layout.overflow);

    bounds.overlaps(sources.viewport) && clip.is_none_or(|clip| bounds.overlaps(clip))
}

/// Push the clip a fragment applies to its descendants, if it clips its
/// overflow, and return the bounds of the clips its descendants paint inside
/// of, with whether a clip layer was pushed.
fn push_clip(
    sources: Sources<'_>,
    key: FragmentKey,
    fragment: &Fragment,
    transforms: Transforms,
    clip: Option<Rect>,
    scene: &mut impl PaintScene,
) -> (Option<Rect>, bool) {
    let axes = sources
        .prepaint
        .get(fragment.source)
        .map_or((false, false), |data| data.clip);

    let Some(layer) = clip_layer(fragment, axes, transforms, sources.viewport) else {
        return (clip, false);
    };

    let transform = transforms.transform;

    // Clipping on both axes follows the rounded corners of the box.
    // TODO:: horizontal, vertical curve clip for round rect.
    let radii = sources
        .prepaint
        .fragment(key)
        .filter(|data| data.border_radius.is_some())
        .map(|data| data.border_radii(fragment));

    match radii.filter(|_| axes == (true, true)) {
        Some(radii) => scene.push_clip_layer(
            peniko::Fill::NonZero,
            transform,
            &layer.to_rounded_rect(radii),
        ),
        None => scene.push_clip_layer(peniko::Fill::NonZero, transform, &layer),
    }

    let bounds = transform.transform_rect_bbox(layer);

    (
        Some(clip.map_or(bounds, |clip| clip.intersect(bounds))),
        true,
    )
}

/// Filter what is painted behind the border box of a layer's owner, with an
/// empty layer clipped to it.
fn draw_backdrop(
    sources: Sources<'_>,
    key: FragmentKey,
    fragment: &Fragment,
    transforms: Transforms,
    clip: Option<Rect>,
    filter: Arc<anyrender::Filter>,
    scene: &mut impl PaintScene,
) {
    let rect = local_rect(fragment);
    let transform = transforms.transform;
    let bounds = transform.transform_rect_bbox(rect);

    // A collapsed box has no area to filter behind.
    if transforms.inverse.is_none()
        || !bounds.overlaps(sources.viewport)
        || clip.is_some_and(|clip| !bounds.overlaps(clip))
    {
        return;
    }

    let border = sources
        .prepaint
        .fragment(key)
        .map_or(rect.to_rounded_rect(0.0), |data| data.border_rect(fragment));

    scene.push_layer(
        peniko::Fill::NonZero,
        BlendMode::default(),
        1.0,
        transform,
        &border,
        None,
        Some(filter),
    );
    scene.pop_layer();
}

/// The border box of a fragment, in its local coordinates.
fn local_rect(fragment: &Fragment) -> Rect {
    Rect::from_origin_size(
        Point::ORIGIN,
        (fragment.layout.size.width, fragment.layout.size.height),
    )
}

/// The clip of a box fragment whose item clips its overflow on the `clip`
/// axes, in its local coordinates: its border box on the clipped axes. Other
/// fragments do not clip. Unclipped axes are bounded by the
/// viewport, mapped back to local coordinates with `transforms`, the
/// fragment's transforms to the scene. Fragments whose scene transform has no
/// inverse push no clip layer.
///
/// Fragments clipping on both axes round the layer with their border radius.
/// Clipping on one axis keeps it square, as every corner then lies on the
/// viewport bound of the unclipped axis.
fn clip_layer(
    fragment: &Fragment,
    (x, y): (bool, bool),
    transforms: Transforms,
    viewport: Rect,
) -> Option<Rect> {
    if !fragment.item.is_box() || !x && !y {
        return None;
    }

    // A collapsed box paints its contents collapsed with it.
    let visible = transforms.inverse?.transform_rect_bbox(viewport);
    let bounds = local_rect(fragment);

    Some(Rect::new(
        if x { bounds.x0 } else { visible.x0 },
        if y { bounds.y0 } else { visible.y0 },
        if x { bounds.x1 } else { visible.x1 },
        if y { bounds.y1 } else { visible.y1 },
    ))
}

/// Bounds of the background, border and outline painted for a fragment's
/// border box `rect`. The border is stroked outside the box, up to its widest
/// side, and the outline past its offset.
fn box_bounds(fragment: &Fragment, data: Option<&LayoutInputData>, rect: Rect) -> Rect {
    let outline = data
        .filter(|data| data.outline_width > 0.0)
        .map_or(0.0, |data| data.outline_offset + data.outline_width);
    let width = border_width(fragment).max(outline);

    rect + Insets::uniform(width)
}

fn border_width(fragment: &Fragment) -> f64 {
    let border = fragment.layout.border;

    border
        .left_top
        .x
        .max(border.left_top.y)
        .max(border.right_bottom.x)
        .max(border.right_bottom.y)
}

/// The source node of a fragment.
fn node(inputs: &LayoutInputTree, fragment: &Fragment) -> Option<NodeKey> {
    inputs.items.get(fragment.source).map(|item| item.node)
}

/// Paint the background, border and outline of a fragment's border box
/// `rect`. The background and border are rounded with the fragment's
/// [border radii](FragmentData::border_radii) from `paint_data`, square at
/// the joins of a split fragment, and the outline with all four corners of
/// its [border radius](FragmentData::border_radius).
#[allow(clippy::too_many_arguments)]
fn draw_box(
    ui: &Ui,
    inputs: &LayoutInputTree,
    fragment: &Fragment,
    data: Option<&LayoutInputData>,
    paint_data: Option<&FragmentData>,
    rect: Rect,
    transform: Affine,
    scene: &mut impl PaintScene,
) {
    let Some(props) = node(inputs, fragment).and_then(|node| ui.props(node)) else {
        return;
    };
    let radii = paint_data.map_or(RoundedRectRadii::from(0.0), |data| {
        data.border_radii(fragment)
    });

    if let Some(fill) = props.get::<Fill>() {
        scene.fill(
            peniko::Fill::NonZero,
            transform,
            &fill.0,
            None,
            &rect.to_rounded_rect(radii),
            AntiAliasing::Enabled,
        );
    }

    let width = border_width(fragment);

    if width > 0.0
        && let Some(fill) = props.get::<BorderFill>()
    {
        scene.stroke(
            &Stroke::new(width),
            transform,
            &fill.0,
            None,
            &rect.inset(-width / 2.0).to_rounded_rect(radii),
            AntiAliasing::Enabled,
        );
    }

    if let Some(data) = data.filter(|data| data.outline_width > 0.0)
        && let Some(fill) = props.get::<OutlineFill>()
    {
        let width = data.outline_width;

        scene.stroke(
            &Stroke::new(width),
            transform,
            &fill.0,
            None,
            &outline_path(
                rect,
                paint_data
                    .and_then(|data| data.border_radius)
                    .unwrap_or_default(),
                data.outline_offset,
                width,
            ),
            AntiAliasing::Enabled,
        );
    }
}

/// The path an outline of `width` is stroked along: each edge of the border
/// box `rect` moved out by `offset` and half the width, rounded with all four
/// border `radii` grown by the same distance. Square corners stay square, and a
/// negative offset shrinks the box no further than its centre.
fn outline_path(rect: Rect, radii: RoundedRectRadii, offset: f64, width: f64) -> RoundedRect {
    let shrink = rect.width().min(rect.height()) / 2.0;
    let distance = (offset + width / 2.0).max(-shrink);
    let grow = |radius: f64| {
        if radius > 0.0 {
            (radius + distance).max(0.0)
        } else {
            0.0
        }
    };
    (rect + Insets::uniform(distance)).to_rounded_rect(RoundedRectRadii::new(
        grow(radii.top_left),
        grow(radii.top_right),
        grow(radii.bottom_right),
        grow(radii.bottom_left),
    ))
}

#[allow(clippy::too_many_arguments)]
fn draw_glyph_run(
    tree: &FragmentTree,
    fragment: &Fragment,
    owner_line: FragmentKey,
    (line, item): (usize, usize),
    rect: Rect,
    transform: Affine,
    scale_factor: f64,
    text_color: &anyrender::Paint,
    visible: &impl Fn(Rect) -> bool,
    scene: &mut impl PaintScene,
) {
    let Some(line) = tree
        .inline_layout(owner_line)
        .and_then(|inline| inline.layout.get(line))
    else {
        return;
    };

    let Some(PositionedLayoutItem::GlyphRun(glyph_run)) = line.items().nth(item) else {
        return;
    };

    let run = glyph_run.run();

    // Glyph ink can extend past the run's line box, such as accents above a
    // tight line height or italic overhangs. One em on each side bounds it.
    let overhang = run.font_size() as f64;

    if !visible(rect.inflate(overhang, overhang)) {
        return;
    }

    let origin = rect.origin();
    // Parley's coordinates wrap the raw `i16` bits Anyrender takes.
    let normalized_coords: &[i16] = TransparentWrapper::peel_slice(run.normalized_coords());

    // Parley's glyphs are positioned within the inline context. Offset them
    // so the run starts at the fragment's local origin.
    let offset_x = origin.x as f32 - glyph_run.offset();
    let offset_y =
        (origin.y + fragment.layout.baseline.unwrap_or_default()) as f32 - glyph_run.baseline();

    // Glyphs are shaped at their logical size but drawn at their physical
    // font size, so they are hinted for the display's pixels. Their positions
    // scale with them, and the display scale is taken out of the transform.
    let scale = scale_factor as f32;

    scene.draw_glyphs(
        run.font(),
        run.font_size() * scale,
        true,
        normalized_coords,
        Vec2::ZERO,
        peniko::StyleRef::Fill(peniko::Fill::NonZero),
        text_color,
        1.0,
        transform * Affine::scale(scale_factor.recip()),
        None,
        glyph_run.positioned_glyphs().map(|glyph| anyrender::Glyph {
            id: glyph.id,
            x: (offset_x + glyph.x) * scale,
            y: (offset_y + glyph.y) * scale,
        }),
        // TODO:: add aa control
        AntiAliasing::Enabled,
    );
}
