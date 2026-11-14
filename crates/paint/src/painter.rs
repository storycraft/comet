//! Painting of fragments into a scene, in the order of their paint layers.

use anyrender::PaintScene;
use bytemuck::TransparentWrapper;
use comet_layout::{
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKind, tree::LayoutInputTree},
};
use comet_tree::{node::NodeKey, ui::Ui};
use kurbo::{Affine, Point, Rect, RoundedRect, Stroke, Vec2};
use parley::PositionedLayoutItem;
use peniko::BlendMode;
use slotmap::SecondaryMap;

use crate::{
    layer::{PaintLayerEntry, PaintLayerKey, tree::PaintLayerTree},
    pre_paint::{FragmentData, tree::PrePaintTree},
    style::{BorderFill, Fill},
};

/// Paints final fragments in the order of their [`PaintLayerTree`], with the
/// [`PrePaintTree`] resolved for them.
#[derive(Default)]
pub struct CometPainter {
    /// Paint state of every fragment under the root layer's owner.
    states: SecondaryMap<FragmentKey, FragmentState>,
    /// Clips fragments paint inside of, each inside its parent.
    clips: Vec<Clip>,
    /// Clips currently pushed to the scene, outermost first.
    pushed: Vec<usize>,
    /// Buffer of the clips a fragment paints inside of, outermost first.
    chain: Vec<usize>,
    /// Fragments to resolve the paint state of.
    pending: Vec<Visit>,
}

/// A fragment to resolve the paint state of.
struct Visit {
    key: FragmentKey,
    /// Line containing the fragment.
    inline_owner: Option<FragmentKey>,
    /// Maps the local coordinates of the fragment's parent to scene
    /// coordinates.
    parent: Transforms,
    /// Clip the fragment paints inside of.
    clip: Option<usize>,
}

/// Where and inside what a fragment paints.
#[derive(Clone, Copy)]
struct FragmentState {
    transforms: Transforms,
    /// Line containing the fragment, which owns the inline layout of its
    /// glyph runs.
    inline_owner: Option<FragmentKey>,
    /// Innermost clip the fragment paints inside of.
    clip: Option<usize>,
}

/// The clip of a box fragment clipping its contents.
struct Clip {
    parent: Option<usize>,
    /// Maps the clip shape to scene coordinates.
    transform: Affine,
    shape: ClipShape,
    /// Scene bounds of the clip, intersected with its parents.
    bounds: Rect,
}

enum ClipShape {
    Rect(Rect),
    Rounded(RoundedRect),
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
    /// multiplied from the root down.
    fn of(tree: &FragmentTree, pre_paint: &PrePaintTree, key: FragmentKey) -> Self {
        let parent = match tree.items.parent(key) {
            Some(parent) => Self::of(tree, pre_paint, parent),
            None => Self::IDENTITY,
        };

        parent.then(&pre_paint[key])
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
    /// `pre_paint` and `layers` must be resolved and built for them.
    ///
    /// # Panics
    ///
    /// Panics if `root` is not in `layers`.
    ///
    /// Layers paint their entries in order, each with the opacity of its
    /// owner fragment: a layer with an opacity below 1 paints into a group
    /// blended with that opacity, and a transparent one is skipped. Each
    /// fragment paints with its resolved data, such as its transform and
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
        let sources = Sources {
            ui,
            inputs,
            tree,
            pre_paint,
            layers,
            viewport,
        };

        self.resolve_states(sources, layers[root].owner);
        self.pushed.clear();
        self.draw_layer(sources, root, scene);
        self.pop_clips(scene);
    }

    /// Resolve the paint state of the fragments under `root`, and the clips
    /// they paint inside of.
    fn resolve_states(&mut self, sources: Sources<'_>, root: FragmentKey) {
        let Sources {
            tree,
            pre_paint,
            viewport,
            ..
        } = sources;

        self.states.clear();
        self.clips.clear();
        self.pending.clear();
        self.pending.push(Visit {
            key: root,
            inline_owner: None,
            parent: match tree.items.parent(root) {
                Some(parent) => Transforms::of(tree, pre_paint, parent),
                None => Transforms::IDENTITY,
            },
            clip: None,
        });

        while let Some(Visit {
            key,
            mut inline_owner,
            parent,
            clip,
        }) = self.pending.pop()
        {
            let fragment = &tree.items[key];

            if matches!(fragment.item, FragmentItem::Line { .. }) {
                inline_owner = Some(key);
            }

            let own = &pre_paint[key];
            let data = &pre_paint[fragment.source];
            let transforms = parent.then(own);

            self.states.insert(
                key,
                FragmentState {
                    transforms,
                    inline_owner,
                    clip,
                },
            );

            // A clipping fragment clips its descendants, not itself.
            let mut child_clip = clip;

            if let Some(layer) = clip_layer(fragment, data.clip, transforms, viewport) {
                // Clipping on both axes follows the rounded corners of the box.
                // TODO:: horizontal, vertical curve clip for round rect.
                let shape = match own.border_radius.filter(|_| data.clip == (true, true)) {
                    Some(radii) => ClipShape::Rounded(layer.to_rounded_rect(radii)),
                    None => ClipShape::Rect(layer),
                };
                let bounds = transforms.transform.transform_rect_bbox(layer);

                child_clip = Some(self.clips.len());
                self.clips.push(Clip {
                    parent: clip,
                    transform: transforms.transform,
                    shape,
                    bounds: clip.map_or(bounds, |clip| self.clips[clip].bounds.intersect(bounds)),
                });
            }

            self.pending
                .extend(tree.items.children(key).rev().map(|child| Visit {
                    key: child,
                    inline_owner,
                    parent: transforms,
                    clip: child_clip,
                }));
        }
    }

    /// Paint the entries of a layer, inside a group with its owner's opacity.
    fn draw_layer(
        &mut self,
        sources: Sources<'_>,
        key: PaintLayerKey,
        scene: &mut impl PaintScene,
    ) {
        let layer = &sources.layers[key];
        let owner = &sources.tree.items[layer.owner];
        let opacity = sources.pre_paint[owner.source].opacity;

        if opacity <= 0.0 {
            return;
        }

        // Clip layers pushed before the group must be popped after it, so the
        // group starts without clips and pushes the ones it needs.
        let group = opacity < 1.0;

        if group {
            self.pop_clips(scene);
            scene.push_layer(
                peniko::Fill::NonZero,
                BlendMode::default(),
                opacity as f32,
                Affine::IDENTITY,
                &sources.viewport,
                None,
                None,
            );
        }

        for &entry in &layer.entries {
            match entry {
                PaintLayerEntry::Fragment(fragment) => {
                    let state = self.states[fragment];

                    self.sync_clips(state.clip, scene);
                    self.draw_fragment(sources, fragment, state, scene);
                }
                PaintLayerEntry::Layer(child) => self.draw_layer(sources, child, scene),
            }
        }

        if group {
            self.pop_clips(scene);
            scene.pop_layer();
        }
    }

    /// Push and pop clip layers, so exactly `clip` and its parents are pushed.
    fn sync_clips(&mut self, clip: Option<usize>, scene: &mut impl PaintScene) {
        self.chain.clear();

        let mut next = clip;

        while let Some(clip) = next {
            self.chain.push(clip);
            next = self.clips[clip].parent;
        }

        self.chain.reverse();

        let kept = self
            .pushed
            .iter()
            .zip(&self.chain)
            .take_while(|(pushed, wanted)| pushed == wanted)
            .count();

        while self.pushed.len() > kept {
            self.pushed.pop();
            scene.pop_layer();
        }

        for &clip in &self.chain[kept..] {
            let Clip {
                transform, shape, ..
            } = &self.clips[clip];

            match shape {
                ClipShape::Rect(rect) => {
                    scene.push_clip_layer(peniko::Fill::NonZero, *transform, rect)
                }
                ClipShape::Rounded(rect) => {
                    scene.push_clip_layer(peniko::Fill::NonZero, *transform, rect)
                }
            }

            self.pushed.push(clip);
        }
    }

    /// Pop every pushed clip layer.
    fn pop_clips(&mut self, scene: &mut impl PaintScene) {
        for _ in self.pushed.drain(..) {
            scene.pop_layer();
        }
    }

    /// Paint one fragment itself, without its descendants.
    fn draw_fragment(
        &self,
        sources: Sources<'_>,
        key: FragmentKey,
        state: FragmentState,
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
        let fragment = &tree.items[key];
        let own = &pre_paint[key];
        let rect = local_rect(fragment);
        let transform = state.transforms.transform;
        let clip = state.clip.map(|clip| self.clips[clip].bounds);
        let visible = |bounds: Rect| {
            let bounds = transform.transform_rect_bbox(bounds);

            bounds.overlaps(viewport) && clip.is_none_or(|clip| bounds.overlaps(clip))
        };

        match fragment.item {
            FragmentItem::Box { .. } => {
                // A standalone text root also owns a layout box. Its paint
                // belongs to its glyph fragments, not to both representations.
                if inputs.items[fragment.source].kind != LayoutInputKind::Text
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

                if let Some(owner) = state.inline_owner {
                    draw_glyph_run(
                        tree,
                        fragment,
                        owner,
                        (line, item),
                        rect,
                        transform,
                        &pre_paint[fragment.source].text_color,
                        &visible,
                        scene,
                    );
                }
            }
        }
    }
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
fn node(inputs: &LayoutInputTree, fragment: &Fragment) -> NodeKey {
    inputs.items[fragment.source].node
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
    let Some(props) = ui.props(node(inputs, fragment)) else {
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
