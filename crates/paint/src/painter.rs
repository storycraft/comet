//! Painting of fragments into a scene, in the order of their paint layers.

use std::sync::Arc;

use anyrender::PaintScene;
use bytemuck::TransparentWrapper;
use comet_layout::{
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKind, tree::LayoutInputTree},
};
use comet_tree::{node::NodeKey, ui::Ui};
use kurbo::{Affine, Point, Rect, Stroke, Vec2};
use parley::PositionedLayoutItem;
use peniko::BlendMode;

use crate::{
    layer::{PaintLayerKey, tree::PaintLayerTree},
    pre_paint::{FragmentData, tree::PrePaintTree},
    style::{BackdropFilter, BorderFill, Fill, Filter, Opacity},
};

/// Paints final fragments in the order of their [`PaintLayerTree`], with the
/// [`PrePaintTree`] resolved for them. Keep it to reuse its buffers across
/// paints.
#[derive(Default)]
pub struct CometPainter {
    /// Fragments to paint in the layer being painted.
    pending: Vec<Visit>,
    /// Ancestors of a child layer's owner, up to the owner of its parent
    /// layer, innermost first.
    chain: Vec<FragmentKey>,
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
    fn of(tree: &FragmentTree, pre_paint: &PrePaintTree, key: FragmentKey) -> Option<Self> {
        let parent = match tree.items.parent(key) {
            Some(parent) => Self::of(tree, pre_paint, parent)?,
            None => Self::IDENTITY,
        };

        Some(parent.then(pre_paint.fragment(key)?))
    }
}

/// What painting reads from.
#[derive(Clone, Copy)]
struct Sources<'a> {
    ui: &'a Ui,
    inputs: &'a LayoutInputTree,
    tree: &'a FragmentTree,
    pre_paint: &'a PrePaintTree,
    layers: &'a PaintLayerTree,
    viewport: Rect,
}

impl CometPainter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Paint the fragments under the owner of `root`, a layer of `layers`.
    /// `inputs` is the input tree the fragments were built from, and
    /// `pre_paint` and `layers` must be resolved and built for them. Paints
    /// nothing if `root` is not in `layers`.
    ///
    /// Each layer paints its owner fragment, then its child layers with a
    /// negative z index, then its other fragments in tree order, then its
    /// other child layers, with the opacity and filters of its owner's item.
    /// A layer with an opacity below 1 or a [`Filter`] paints into a group,
    /// blended with that opacity and filtered in the owner's local
    /// coordinates. A transparent layer is skipped, and so is the group of a
    /// layer whose owner collapses. A [`BackdropFilter`] filters what is
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
    /// Fragments painting entirely outside `viewport` or their clip, in scene
    /// coordinates, are skipped. Fragments touching their edges are painted.
    /// Their descendants are still painted, since children may paint outside
    /// their parents.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        tree: &FragmentTree,
        pre_paint: &PrePaintTree,
        layers: &PaintLayerTree,
        root: PaintLayerKey,
        viewport: Rect,
        scene: &mut impl PaintScene,
    ) {
        let Some(item) = layers.layers.get(root) else {
            return;
        };
        let parent = match tree.items.parent(item.owner) {
            Some(parent) => Transforms::of(tree, pre_paint, parent),
            None => Some(Transforms::IDENTITY),
        };
        let Some(parent) = parent else {
            return;
        };

        let sources = Sources {
            ui,
            inputs,
            tree,
            pre_paint,
            layers,
            viewport,
        };

        self.pending.clear();
        self.draw_layer(sources, root, parent, None, scene);
    }

    /// Paint a layer, after its owner's backdrop filter and inside a group
    /// with its owner's opacity and filter. `parent` maps the local
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
        let Some(owner) = sources.layers.layers.get(key).map(|item| item.owner) else {
            return;
        };
        let (Some(fragment), Some(own)) = (
            sources.tree.items.get(owner),
            sources.pre_paint.fragment(owner),
        ) else {
            return;
        };

        let props = sources
            .inputs
            .items
            .get(fragment.source)
            .and_then(|item| sources.ui.props(item.node));
        let opacity = props.map_or(1.0, |props| {
            let Opacity(opacity) = props.get_or_default();
            opacity.clamp(0.0, 1.0)
        });

        if opacity <= 0.0 {
            return;
        }

        let filter = props
            .and_then(|props| props.get_cloned())
            .map(|Filter(filter)| filter);
        let backdrop_filter = props
            .and_then(|props| props.get_cloned())
            .map(|BackdropFilter(filter)| filter);
        let transforms = parent.then(own);

        if let Some(backdrop_filter) = backdrop_filter {
            draw_backdrop(
                sources,
                fragment,
                own,
                transforms,
                clip,
                backdrop_filter,
                scene,
            );
        }

        let group = opacity < 1.0 || filter.is_some();

        if group {
            // A collapsed owner paints nothing, and its contents with it.
            let Some(inverse) = transforms.inverse else {
                return;
            };

            scene.push_layer(
                peniko::Fill::NonZero,
                BlendMode::default(),
                opacity as f32,
                transforms.transform,
                &inverse.transform_rect_bbox(sources.viewport),
                filter,
                None,
            );
        }

        // The owner paints outside its own clip, and its descendants inside
        // it, child layers included.
        self.draw_fragment(sources, owner, transforms, None, clip, scene);
        let (clip, clipped) = push_clip(sources, fragment, own, transforms, clip, scene);

        for child in sources.layers.layers.children(key) {
            if sources.layers.layers[child].z_index >= 0 {
                break;
            }

            self.draw_child(sources, child, owner, transforms, clip, scene);
        }

        self.draw_contents(sources, owner, transforms, clip, scene);

        for child in sources.layers.layers.children(key) {
            if sources.layers.layers[child].z_index >= 0 {
                self.draw_child(sources, child, owner, transforms, clip, scene);
            }
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
                (sources.tree.items.get(key), sources.pre_paint.fragment(key))
            else {
                continue;
            };

            transforms = transforms.then(own);

            let clipped;
            (clip, clipped) = push_clip(sources, fragment, own, transforms, clip, scene);
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
                (sources.tree.items.get(key), sources.pre_paint.fragment(key))
            else {
                continue;
            };

            if matches!(fragment.item, FragmentItem::Line { .. }) {
                inline_owner = Some(key);
            }

            let transforms = parent.then(own);
            self.draw_fragment(sources, key, transforms, inline_owner, clip, scene);

            // A clipping fragment clips its descendants, not itself.
            let (child_clip, clipped) = push_clip(sources, fragment, own, transforms, clip, scene);

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

    /// Paint one fragment itself, without its descendants, inside the clips
    /// bounded by `clip`.
    fn draw_fragment(
        &self,
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
            pre_paint,
            viewport,
            ..
        } = sources;
        let (Some(fragment), Some(own)) = (tree.items.get(key), pre_paint.fragment(key)) else {
            return;
        };
        let rect = local_rect(fragment);
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
                    && visible(box_bounds(fragment, rect))
                {
                    draw_box(ui, inputs, fragment, own, rect, transform, scene);
                }
            }
            FragmentItem::Inline { .. } => {
                if visible(box_bounds(fragment, rect)) {
                    draw_box(ui, inputs, fragment, own, rect, transform, scene);
                }
            }
            // Anonymous blocks only position their lines.
            FragmentItem::AnonymousBlock { .. } => {}
            FragmentItem::Line { .. } => {}
            FragmentItem::GlyphRun { line, item } => {
                if visible(box_bounds(fragment, rect)) {
                    draw_box(ui, inputs, fragment, own, rect, transform, scene);
                }

                if let Some(owner) = inline_owner
                    && let Some(data) = pre_paint.get(fragment.source)
                {
                    draw_glyph_run(
                        tree,
                        fragment,
                        owner,
                        (line, item),
                        rect,
                        transform,
                        &data.text_color,
                        &visible,
                        scene,
                    );
                }
            }
        }
    }
}

/// Push the clip a fragment applies to its descendants, if it clips its
/// overflow, and return the bounds of the clips its descendants paint inside
/// of, with whether a clip layer was pushed.
fn push_clip(
    sources: Sources<'_>,
    fragment: &Fragment,
    own: &FragmentData,
    transforms: Transforms,
    clip: Option<Rect>,
    scene: &mut impl PaintScene,
) -> (Option<Rect>, bool) {
    let axes = sources
        .pre_paint
        .get(fragment.source)
        .map_or((false, false), |data| data.clip);

    let Some(layer) = clip_layer(fragment, axes, transforms, sources.viewport) else {
        return (clip, false);
    };

    let transform = transforms.transform;

    // Clipping on both axes follows the rounded corners of the box.
    // TODO:: horizontal, vertical curve clip for round rect.
    match own.border_radius.filter(|_| axes == (true, true)) {
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
    fragment: &Fragment,
    own: &FragmentData,
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

    let radii = own.border_radius.unwrap_or_default();

    scene.push_layer(
        peniko::Fill::NonZero,
        BlendMode::default(),
        1.0,
        transform,
        &rect.to_rounded_rect(radii),
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

/// Bounds of the background and border painted for a fragment's border box.
/// The border is stroked outside the box, up to its widest side.
fn box_bounds(fragment: &Fragment, rect: Rect) -> Rect {
    let width = border_width(fragment);

    rect.inflate(width, width)
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

/// Paint the background and border of a fragment's border box `rect`, with
/// its resolved corner radii.
fn draw_box(
    ui: &Ui,
    inputs: &LayoutInputTree,
    fragment: &Fragment,
    own: &FragmentData,
    rect: Rect,
    transform: Affine,
    scene: &mut impl PaintScene,
) {
    let Some(props) = node(inputs, fragment).and_then(|node| ui.props(node)) else {
        return;
    };

    let radii = own.border_radius.unwrap_or_default();

    if let Some(fill) = props.get::<Fill>() {
        scene.fill(
            peniko::Fill::NonZero,
            transform,
            &fill.0,
            None,
            &rect.to_rounded_rect(radii),
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
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_glyph_run(
    tree: &FragmentTree,
    fragment: &Fragment,
    owner_line: FragmentKey,
    (line, item): (usize, usize),
    rect: Rect,
    transform: Affine,
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

    scene.draw_glyphs(
        run.font(),
        run.font_size(),
        true,
        normalized_coords,
        Vec2::ZERO,
        peniko::StyleRef::Fill(peniko::Fill::NonZero),
        text_color,
        1.0,
        transform,
        None,
        glyph_run.positioned_glyphs().map(|glyph| anyrender::Glyph {
            id: glyph.id,
            x: offset_x + glyph.x,
            y: offset_y + glyph.y,
        }),
    );
}
