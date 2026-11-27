//! Painting of fragments into a scene, in the order of their paint layers.

use std::sync::Arc;

use anyrender::PaintScene;
use anyrender::{AntiAliasing, PaintRef};
use bytemuck::TransparentWrapper;
use comet_layout::{
    fragment::{
        BoxFragment, BoxFragmentKey, FragmentGeometry, FragmentRef, inline::InlineItem,
        inline::InlineItemKind, tree::FragmentTree,
    },
    input::tree::LayoutInputTree,
};
use comet_tree::ui::{Props, Ui};
use kurbo::{
    Affine, BezPath, Insets, Point, Rect, RoundedRect, RoundedRectRadii, Shape, Stroke, Vec2,
};
use parley::PositionedLayoutItem;
use peniko::BlendMode;

use crate::{
    layer::{
        PaintLayerKey,
        tree::{PaintLayerTree, layer_owner},
    },
    prepaint::{
        FragmentData, JoinedSides, LayoutInputData,
        fragment::{BoxRole, for_each_paint_child_on_lines, paint_props, piece_of_part},
        tree::PrepaintTree,
    },
    selection::rects::SelectionRects,
    style::{self, Background, BorderFill, BoxDecorationBreak, OutlineFill, Paint},
};

/// Paints final fragments in the order of their [`PaintLayerTree`], with the
/// [`PrepaintTree`] resolved for them. Keep it to reuse its buffers across
/// paints.
#[derive(Default)]
pub struct CometPainter {
    /// Fragments to paint in the layer being painted.
    pending: Vec<Visit>,
    /// Ancestors of a child layer's owner, up to a box of its parent layer
    /// with known transforms, innermost first.
    chain: Vec<BoxFragmentKey>,
    /// Selection area behind the glyph run being painted.
    selection: Vec<Rect>,
}

/// A step of painting the fragments of a layer.
enum Visit {
    /// Paint a fragment and queue its children.
    Fragment {
        fragment: FragmentRef,
        /// Maps the coordinates of the contents of the box the fragment is
        /// placed in to scene coordinates: its parent box, or the owner of
        /// the inline layout holding an inline item.
        parent: Transforms,
        /// Scene bounds of the clips the fragment paints inside of.
        clip: Option<Rect>,
    },
    /// Pop the clip layer of a fragment's contents.
    PopClip,
}

/// A transform from local box coordinates to scene coordinates, and its
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

    /// The transforms of the box `key` to its parent box, from `layers`:
    /// see [`PaintLayerTree::transform`]. `None` if it is not in `tree`.
    fn own(tree: &FragmentTree, layers: &PaintLayerTree, key: BoxFragmentKey) -> Option<Self> {
        Some(Self {
            transform: layers.transform(tree, key)?,
            inverse: layers.inverse_transform(tree, key),
        })
    }

    /// The transforms of a box whose parent maps to the scene with `self`,
    /// and which maps to its parent with `own`.
    fn then(self, own: Self) -> Self {
        Self {
            transform: self.transform * own.transform,
            inverse: own
                .inverse
                .zip(self.inverse)
                .map(|(own, parent)| own * parent),
        }
    }

    /// The transforms of the parent of a box mapping to the scene with
    /// `self`, undoing `own`, its transforms to its parent. `None` if it has
    /// no inverse.
    fn undo(self, own: Self) -> Option<Self> {
        Some(Self {
            transform: self.transform * own.inverse?,
            inverse: self.inverse.map(|inverse| own.transform * inverse),
        })
    }

    /// The transforms of the contents of a box mapping to the scene with
    /// `self`, scrolled by `offset`: its descendants move against the
    /// scroll, while the box and its clip stay in place.
    fn scrolled(self, offset: Vec2) -> Self {
        if offset == Vec2::ZERO {
            return self;
        }

        Self {
            transform: self.transform * Affine::translate(-offset),
            inverse: self
                .inverse
                .map(|inverse| Affine::translate(offset) * inverse),
        }
    }

    /// The transforms of the contents of the box `key`, mapping to the scene
    /// with `self`: scrolled by the scroll offset of the layer it owns.
    fn contents(self, layers: &PaintLayerTree, key: BoxFragmentKey) -> Self {
        self.scrolled(layers.scroll_offset(key))
    }

    /// The transforms mapping the local coordinates of `key` to the scene,
    /// multiplied from the root down with the scroll offsets of its
    /// ancestors, or `None` if a box on the way is not resolved.
    fn of(tree: &FragmentTree, layers: &PaintLayerTree, key: BoxFragmentKey) -> Option<Self> {
        let parent = match tree.boxes.parent(key) {
            Some(parent) => Self::of(tree, layers, parent)?.contents(layers, parent),
            None => Self::IDENTITY,
        };

        Some(parent.then(Self::own(tree, layers, key)?))
    }
}

/// Boxes of a layer whose contents have known transforms, which the boxes
/// painting in the layer are placed from: the owner of a box layer, or the
/// boxes holding the inline content of an inline element's layer.
#[derive(Clone, Copy)]
struct Bases {
    boxes: [(BoxFragmentKey, Transforms); 2],
    len: usize,
}

impl Bases {
    fn one(key: BoxFragmentKey, transforms: Transforms) -> Self {
        Self {
            boxes: [(key, transforms); 2],
            len: 1,
        }
    }

    fn get(&self, key: BoxFragmentKey) -> Option<Transforms> {
        self.boxes[..self.len]
            .iter()
            .find(|(base, _)| *base == key)
            .map(|&(_, transforms)| transforms)
    }
}

/// What painting reads from.
#[derive(Clone, Copy)]
struct Sources<'a> {
    ui: &'a Ui,
    inputs: &'a LayoutInputTree,
    tree: &'a FragmentTree,
    prepaint: &'a PrepaintTree,
    /// Rectangles behind the selected text.
    selection: &'a SelectionRects,
    layers: &'a PaintLayerTree,
    /// The scene's viewport, in physical pixels.
    viewport: Rect,
    /// Physical pixels per logical layout unit.
    scale_factor: f64,
}

impl Sources<'_> {
    /// The transforms of the box `key` to its parent box.
    fn own(&self, key: BoxFragmentKey) -> Option<Transforms> {
        Transforms::own(self.tree, self.layers, key)
    }

    /// The transforms of the contents of `key`, placed from `bases`: the
    /// transforms of a base, or those of the boxes down from one. `None` if
    /// `key` is not under a base, or a box on the way is not resolved.
    fn contents_of(&self, bases: &Bases, key: BoxFragmentKey) -> Option<Transforms> {
        if let Some(transforms) = bases.get(key) {
            return Some(transforms);
        }

        let parent = self.contents_of(bases, self.tree.boxes.parent(key)?)?;

        Some(parent.then(self.own(key)?).contents(self.layers, key))
    }

    /// The transforms of an inline part box, and of the contents of the box
    /// it is placed in, placed from `bases`.
    fn part_transforms(
        &self,
        bases: &Bases,
        part: BoxFragmentKey,
    ) -> Option<(Transforms, Transforms)> {
        let parent = self.contents_of(bases, self.tree.boxes.parent(part)?)?;

        Some((parent.then(self.own(part)?), parent))
    }

    /// Whether a fragment starts a layer of its own, painting it with its
    /// descendants, and every piece of its inline element.
    fn starts_layer(&self, fragment: FragmentRef) -> bool {
        layer_owner(self.tree, fragment).is_some_and(|owner| self.layers.layer_of(owner).is_some())
    }

    /// Whether `bounds`, in coordinates mapped to the scene by `transform`,
    /// may paint inside the viewport and `clip`.
    fn visible(&self, transform: Affine, bounds: Rect, clip: Option<Rect>) -> bool {
        let bounds = transform.transform_rect_bbox(bounds);

        bounds.overlaps(self.viewport) && clip.is_none_or(|clip| bounds.overlaps(clip))
    }
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
    /// Each layer paints its owner, then its child layers with a negative z
    /// index, then its other fragments in paint order, then its other child
    /// layers, with the opacity and filters of its owner's item. The layer
    /// of an inline element paints the inline part boxes of all of its
    /// pieces as its owner, and their items as its fragments. A layer with
    /// an opacity below 1, a [`Filter`](crate::style::Filter) or a
    /// [`MixBlend`](crate::style::MixBlend) paints into a group, blended with
    /// that opacity and blend mode and filtered in the owner's local
    /// coordinates. A transparent layer is skipped, and so is the group of a
    /// layer whose owner collapses. A
    /// [`BackdropFilter`](crate::style::BackdropFilter) filters what is
    /// painted behind the owner's border box, rounded with its border radius,
    /// or behind each piece of an inline element, before the layer paints.
    ///
    /// Each box paints with its resolved data, such as its transform and
    /// border radius. Box transforms multiply down the box tree, starting
    /// with the transforms of the root's ancestors. Inline items paint in the
    /// coordinates of the box owning their inline layout: glyph runs with
    /// the text color inherited by their text item, and pieces of inline elements
    /// with their inline part boxes. Items placing no box paint no
    /// background, border or outline.
    ///
    /// Each glyph run paints the [`SelectionRects`] of its text item across
    /// it with the item's selection background, then its glyphs, then the
    /// selected glyphs again in the item's selection color.
    ///
    /// A principal box clipping its overflow clips its descendants to its
    /// border box, rounded with its border radius, including those painting
    /// in other layers. Clips of the root's ancestors do not apply.
    ///
    /// A scroll container starts a layer, moving its descendants, including
    /// those painting in other layers, against its clamped
    /// [scroll offset](crate::layer::PaintLayerItem::scroll). The
    /// container itself, its background, border, outline and clip stay in
    /// place. Scroll offsets of the root's ancestors apply too.
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
    /// fragment's [overflow](comet_layout::fragment::FragmentGeometry::overflow)
    /// is outside too, since children may paint outside their parents, and
    /// so are the items of lines out of view. Child layers are visited on
    /// their own, so they paint wherever their transforms place them.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        tree: &FragmentTree,
        prepaint: &PrepaintTree,
        selection: &SelectionRects,
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
        let parent = match tree.boxes.parent(item.owner) {
            // The owner sits in its parent's scrolled contents.
            Some(parent) => Transforms::of(tree, layers, parent)
                .map(|transforms| transforms.contents(layers, parent)),
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
            selection,
            layers,
            viewport,
            scale_factor,
        };

        self.pending.clear();
        self.draw_layer(sources, root, parent, None, scene);
    }

    /// Paint a layer, after its owner's backdrop filter and inside a group
    /// with its owner's opacity, filter and blend mode. `parent` maps the
    /// contents of the owner's parent box to the scene, and `clip` bounds the
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
        let (Some(fragment), Some(own)) = (sources.tree.get(owner), sources.own(owner)) else {
            return;
        };

        let opacity = layer.opacity;

        if opacity <= 0.0 {
            return;
        }

        let filter = layer.filter.clone();
        let backdrop_filter = layer.backdrop_filter.clone();
        let blend_mode = layer.blend_mode;
        let transforms = parent.then(own);
        let inline = BoxRole::of(fragment) == BoxRole::InlinePart;

        // The pieces of an inline element are placed in the box holding its
        // inline content: the box owning the inline layout of the first, or
        // the box whose runs hold them.
        let bases = if inline {
            let tree = sources.tree;
            let Some(holder) = tree.boxes.parent(owner) else {
                return;
            };
            let mut bases = Bases::one(holder, parent);

            if let Some(holder_fragment) = tree.get(holder)
                && BoxRole::of(holder_fragment) == BoxRole::Anonymous
                && let Some(outer) = tree.boxes.parent(holder)
                && let Some(outer_transforms) = sources.own(holder).and_then(|own| parent.undo(own))
            {
                bases.boxes[1] = (outer, outer_transforms);
                bases.len = 2;
            }

            bases
        } else {
            Bases::one(owner, transforms.contents(sources.layers, owner))
        };

        if let Some(backdrop_filter) = backdrop_filter {
            if inline {
                for part in sources.tree.split_boxes(owner) {
                    if let Some(((part_transforms, _), part_fragment)) = sources
                        .part_transforms(&bases, part)
                        .zip(sources.tree.get(part))
                    {
                        draw_backdrop(
                            sources,
                            part,
                            part_fragment,
                            part_transforms,
                            clip,
                            backdrop_filter.clone(),
                            scene,
                        );
                    }
                }
            } else {
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
        }

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
        // it, child layers included. Inline elements do not clip.
        let (clip, clipped) = if inline {
            for part in sources.tree.split_boxes(owner) {
                if let Some(((part_transforms, _), part_fragment)) = sources
                    .part_transforms(&bases, part)
                    .zip(sources.tree.get(part))
                {
                    draw_box_fragment(sources, part, part_fragment, part_transforms, clip, scene);
                }
            }

            (clip, false)
        } else {
            draw_box_fragment(sources, owner, fragment, transforms, clip, scene);
            push_clip(sources, owner, fragment, transforms, clip, scene)
        };
        // A box layer's descendants, child layers included, scroll with the
        // owner's contents; its clip does not.
        let bases = if inline {
            bases
        } else {
            Bases::one(owner, transforms.contents(sources.layers, owner))
        };

        // The first child layer with a non-negative z index, where painting
        // child layers resumes after the contents.
        let mut non_negative = None;

        for child in sources.layers.layers.children(key) {
            if sources.layers.layers[child].z_index >= 0 {
                non_negative = Some(child);
                break;
            }

            self.draw_child(sources, child, &bases, clip, scene);
        }

        if inline {
            // Each piece paints its items in turn.
            for part in sources.tree.split_boxes(owner) {
                let (Some(piece), Some((part_transforms, parent)), Some(part_fragment)) = (
                    piece_of_part(sources.tree, part),
                    sources.part_transforms(&bases, part),
                    sources.tree.get(part),
                ) else {
                    continue;
                };

                if sources.visible(
                    part_transforms.transform,
                    part_fragment.layout.geometry.overflow,
                    clip,
                ) {
                    self.draw_contents(sources, piece, parent, clip, scene);
                }
            }
        } else if sources.visible(
            transforms.transform,
            fragment.layout.geometry.overflow,
            clip,
        ) && let Some(contents) = bases.get(owner)
        {
            self.draw_contents(sources, FragmentRef::Box(owner), contents, clip, scene);
        }

        for child in sources.layers.layers.cursor(non_negative) {
            self.draw_child(sources, child, &bases, clip, scene);
        }

        if clipped {
            scene.pop_layer();
        }

        if group {
            scene.pop_layer();
        }
    }

    /// Paint a child layer of a layer whose boxes in `bases` have known
    /// transforms, inside the clips of the boxes between its owner and a base
    /// and scrolled by them. `clip` bounds the clips the bases' contents paint
    /// inside of.
    fn draw_child(
        &mut self,
        sources: Sources<'_>,
        child: PaintLayerKey,
        bases: &Bases,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) {
        let Some(child_owner) = sources.layers.layers.get(child).map(|item| item.owner) else {
            return;
        };

        // Boxes between the owner and a base, innermost first.
        self.chain.clear();
        let mut next = sources.tree.boxes.parent(child_owner);

        let base = loop {
            // An outdated layer not under a base paints nowhere.
            let Some(key) = next else {
                return;
            };

            if let Some(transforms) = bases.get(key) {
                break transforms;
            }

            self.chain.push(key);
            next = sources.tree.boxes.parent(key);
        };

        let mut transforms = base;
        let mut clip = clip;
        let mut pushed = 0;

        for &key in self.chain.iter().rev() {
            let (Some(fragment), Some(own)) = (sources.tree.get(key), sources.own(key)) else {
                continue;
            };

            transforms = transforms.then(own);

            let clipped;
            (clip, clipped) = push_clip(sources, key, fragment, transforms, clip, scene);
            pushed += usize::from(clipped);
            transforms = transforms.contents(sources.layers, key);
        }

        self.draw_layer(sources, child, transforms, clip, scene);

        for _ in 0..pushed {
            scene.pop_layer();
        }
    }

    /// Paint the descendants of `fragment` in paint order, skipping the
    /// fragments starting child layers and their descendants. `parent` maps
    /// the coordinates the children of `fragment` are placed in to the scene:
    /// the scrolled contents of a box, or those of the owner of the inline
    /// layout holding an inline item.
    fn draw_contents(
        &mut self,
        sources: Sources<'_>,
        fragment: FragmentRef,
        parent: Transforms,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) {
        let base = self.pending.len();
        self.push_children(sources, fragment, parent, clip);

        while self.pending.len() > base {
            let Some(visit) = self.pending.pop() else {
                break;
            };

            let (fragment, parent, clip) = match visit {
                Visit::Fragment {
                    fragment,
                    parent,
                    clip,
                } => (fragment, parent, clip),
                Visit::PopClip => {
                    scene.pop_layer();
                    continue;
                }
            };

            // Child layers paint on their own, and the layer of an inline
            // element with every piece of it and their items.
            if sources.starts_layer(fragment) {
                continue;
            }

            match fragment {
                FragmentRef::Box(key) => {
                    let (Some(box_fragment), Some(own)) = (sources.tree.get(key), sources.own(key))
                    else {
                        continue;
                    };

                    let transforms = parent.then(own);
                    draw_box_fragment(sources, key, box_fragment, transforms, clip, scene);

                    // Nothing below paints in view: skip the whole subtree.
                    if !sources.visible(
                        transforms.transform,
                        box_fragment.layout.geometry.overflow,
                        clip,
                    ) {
                        continue;
                    }

                    // A clipping box clips its descendants, not itself.
                    let (child_clip, clipped) =
                        push_clip(sources, key, box_fragment, transforms, clip, scene);

                    if clipped {
                        self.pending.push(Visit::PopClip);
                    }

                    let contents = transforms.contents(sources.layers, key);
                    self.push_children(sources, fragment, contents, child_clip);
                }
                FragmentRef::Inline { owner, index } => {
                    let Some(inline) = sources.tree.inline_layout(owner) else {
                        continue;
                    };
                    let Some(item) = inline.items.get(index as usize) else {
                        continue;
                    };

                    match item.kind {
                        InlineItemKind::Run { line, run } => {
                            self.draw_glyph_run(
                                sources,
                                owner,
                                item,
                                (line as usize, run as usize),
                                parent,
                                clip,
                                scene,
                            );
                        }
                        InlineItemKind::Box { child } => {
                            // A piece of an inline element paints its inline
                            // part box under its items.
                            if let Some(part) = child
                                && let (Some(part_fragment), Some(own)) =
                                    (sources.tree.get(part), sources.own(part))
                            {
                                let transforms = parent.then(own);
                                draw_box_fragment(
                                    sources,
                                    part,
                                    part_fragment,
                                    transforms,
                                    clip,
                                    scene,
                                );

                                if !sources.visible(
                                    transforms.transform,
                                    item.geometry.overflow,
                                    clip,
                                ) {
                                    continue;
                                }
                            }

                            // Its items are placed in the same box.
                            self.push_children(sources, fragment, parent, clip);
                        }
                        InlineItemKind::Line { .. } => {
                            self.push_children(sources, fragment, parent, clip);
                        }
                    }
                }
            }
        }
    }

    /// Queue the children of `fragment` in paint order, placed with
    /// `parent`, skipping the items of lines out of view.
    fn push_children(
        &mut self,
        sources: Sources<'_>,
        fragment: FragmentRef,
        parent: Transforms,
        clip: Option<Rect>,
    ) {
        let start = self.pending.len();

        for_each_paint_child_on_lines(
            sources.tree,
            fragment,
            |line| sources.visible(parent.transform, line.geometry.extent(), clip),
            |child| {
                self.pending.push(Visit::Fragment {
                    fragment: child,
                    parent,
                    clip,
                });
            },
        );

        // Popped from the last.
        self.pending[start..].reverse();
    }

    /// Paint the glyph run `item` of the inline layout of `owner`, the run
    /// `run` of its line `line`, in the coordinates of the contents of `owner`,
    /// which `parent` maps to the scene. `None` if nothing was drawn: the run
    /// is missing, out of view, or its item has no resolved text color.
    #[allow(clippy::too_many_arguments)]
    fn draw_glyph_run(
        &mut self,
        sources: Sources<'_>,
        owner: BoxFragmentKey,
        item: &InlineItem,
        (line, run): (usize, usize),
        parent: Transforms,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) -> Option<()> {
        let inline = sources.tree.inline_layout(owner)?;
        let PositionedLayoutItem::GlyphRun(glyph_run) =
            inline.layout.get(line)?.items().nth(run)?
        else {
            return None;
        };

        let font_run = glyph_run.run();
        let geometry = &item.geometry;
        let rect = local_rect(geometry) + Vec2::new(geometry.location.x, geometry.location.y);

        // Glyph ink can extend past the run's line box, such as accents above a
        // tight line height or italic overhangs. One em on each side bounds it.
        let overhang = font_run.font_size() as f64;

        if !sources.visible(parent.transform, rect.inflate(overhang, overhang), clip) {
            return None;
        }

        // The brush of the run's style is the text item it was shaped from.
        // Runs without one belong to the owner.
        let key = glyph_run
            .style()
            .brush
            .or_else(|| sources.tree.get(owner).map(|owner| owner.source))?;

        // Resolved by the pre-paint step. Runs of items it missed are skipped.
        let data = sources.prepaint.get(key)?;
        let anti_aliasing = paint_props(sources.ui, sources.inputs, key)
            .map_or(AntiAliasing::Enabled, anti_aliasing);

        // Parley's coordinates wrap the raw `i16` bits Anyrender takes.
        let normalized_coords: &[i16] =
            TransparentWrapper::peel_slice(font_run.normalized_coords());

        // Parley's glyphs are positioned within the inline layout, whose origin
        // sits at the owner's content box.
        let origin = inline.origin();
        let (offset_x, offset_y) = (origin.x as f32, origin.y as f32);

        // The selection area of the item behind this run: its rectangles
        // across the run, from the top to the bottom of their lines.
        self.selection.clear();
        self.selection.extend(
            sources
                .selection
                .rects(key)
                .iter()
                .map(|area| *area + origin)
                .filter(|area| area.y0 < rect.y1 && area.y1 > rect.y0)
                .map(|area| Rect::new(area.x0.max(rect.x0), area.y0, area.x1.min(rect.x1), area.y1))
                .filter(|area| area.x0 < area.x1),
        );

        if let Some(background) = scene_paint(&data.selection_background) {
            for area in &self.selection {
                scene.fill(
                    peniko::Fill::NonZero,
                    parent.transform,
                    background.clone(),
                    None,
                    area,
                    anti_aliasing,
                );
            }
        }

        // Glyphs are shaped at their logical size but drawn at their physical
        // font size, so they are hinted for the display's pixels. Their positions
        // scale with them, and the display scale is taken out of the transform.
        let scale_factor = sources.scale_factor;
        let scale = scale_factor as f32;
        let draw = |scene: &mut _, paint: &Paint| {
            let Some(paint) = scene_paint(paint) else {
                return;
            };

            draw_glyphs(
                scene,
                font_run.font(),
                font_run.font_size() * scale,
                normalized_coords,
                paint,
                parent.transform * Affine::scale(scale_factor.recip()),
                glyph_run.positioned_glyphs().map(|glyph| anyrender::Glyph {
                    id: glyph.id,
                    x: (offset_x + glyph.x) * scale,
                    y: (offset_y + glyph.y) * scale,
                }),
                anti_aliasing,
            )
        };

        draw(scene, &data.color);

        // Selected glyphs are drawn again over themselves, in the selection
        // color, clipped to the selection area.
        if !self.selection.is_empty() {
            let mut shape = BezPath::new();
            for area in &self.selection {
                shape.extend(area.path_elements(0.1));
            }

            scene.push_clip_layer(peniko::Fill::NonZero, parent.transform, &shape);
            draw(scene, &data.selection_color);
            scene.pop_layer();
        }

        Some(())
    }
}

/// Draw `glyphs` of `font` filled with `paint`, with `transform`.
#[allow(clippy::too_many_arguments)]
fn draw_glyphs(
    scene: &mut impl PaintScene,
    font: &peniko::FontData,
    font_size: f32,
    normalized_coords: &[i16],
    paint: PaintRef<'_>,
    transform: Affine,
    glyphs: impl Iterator<Item = anyrender::Glyph> + Clone,
    anti_aliasing: AntiAliasing,
) {
    scene.draw_glyphs(
        font,
        font_size,
        true,
        normalized_coords,
        Vec2::ZERO,
        peniko::StyleRef::Fill(peniko::Fill::NonZero),
        paint,
        1.0,
        transform,
        None,
        glyphs,
        anti_aliasing,
    );
}

/// Paint the background, border and outline of a box, without its
/// descendants, inside the clips bounded by `clip`. Anonymous boxes paint
/// nothing, and neither do the boxes of text items, which have no paint
/// data.
fn draw_box_fragment(
    sources: Sources<'_>,
    key: BoxFragmentKey,
    fragment: &BoxFragment,
    transforms: Transforms,
    clip: Option<Rect>,
    scene: &mut impl PaintScene,
) {
    if !BoxRole::of(fragment).decorates() {
        return;
    }

    let data = sources.prepaint.get(fragment.source);
    let geometry = &fragment.layout.geometry;
    let rect = local_rect(geometry);

    if sources.visible(transforms.transform, box_bounds(geometry, data, rect), clip) {
        draw_box(
            sources,
            fragment,
            data,
            sources.prepaint.fragment(key),
            rect,
            transforms.transform,
            scene,
        );
    }
}

/// Push the clip a box applies to its descendants, if it clips its
/// overflow, and return the bounds of the clips its descendants paint inside
/// of, with whether a clip layer was pushed.
fn push_clip(
    sources: Sources<'_>,
    key: BoxFragmentKey,
    fragment: &BoxFragment,
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
        .map(|data| {
            data.border_radii(
                fragment,
                sources.prepaint.box_decoration_break(fragment.source),
            )
        });

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

/// Filter what is painted behind the border box of a box, with an empty
/// layer clipped to it.
fn draw_backdrop(
    sources: Sources<'_>,
    key: BoxFragmentKey,
    fragment: &BoxFragment,
    transforms: Transforms,
    clip: Option<Rect>,
    filter: Arc<anyrender::Filter>,
    scene: &mut impl PaintScene,
) {
    let rect = local_rect(&fragment.layout.geometry);
    let transform = transforms.transform;

    // A collapsed box has no area to filter behind.
    if transforms.inverse.is_none() || !sources.visible(transform, rect, clip) {
        return;
    }

    let border = sources
        .prepaint
        .fragment(key)
        .map_or(rect.to_rounded_rect(0.0), |data| {
            data.border_rect(
                fragment,
                sources.prepaint.box_decoration_break(fragment.source),
            )
        });

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
fn local_rect(geometry: &FragmentGeometry) -> Rect {
    Rect::from_origin_size(Point::ORIGIN, (geometry.size.width, geometry.size.height))
}

/// The clip of a principal box whose item clips its overflow on the `clip`
/// axes, in its local coordinates: its border box on the clipped axes. Other
/// boxes do not clip. Unclipped axes are bounded by the viewport, mapped back
/// to local coordinates with `transforms`, the box's transforms to the
/// scene. Boxes whose scene transform has no inverse push no clip layer.
///
/// Boxes clipping on both axes round the layer with their border radius.
/// Clipping on one axis keeps it square, as every corner then lies on the
/// viewport bound of the unclipped axis.
fn clip_layer(
    fragment: &BoxFragment,
    (x, y): (bool, bool),
    transforms: Transforms,
    viewport: Rect,
) -> Option<Rect> {
    if !BoxRole::of(fragment).is_principal() || !x && !y {
        return None;
    }

    // A collapsed box paints its contents collapsed with it.
    let visible = transforms.inverse?.transform_rect_bbox(viewport);
    let bounds = local_rect(&fragment.layout.geometry);

    Some(Rect::new(
        if x { bounds.x0 } else { visible.x0 },
        if y { bounds.y0 } else { visible.y0 },
        if x { bounds.x1 } else { visible.x1 },
        if y { bounds.y1 } else { visible.y1 },
    ))
}

/// Bounds of the background, border and outline painted for a box's border
/// box `rect`. The border is stroked outside the box, up to its widest side,
/// and the outline past its offset.
fn box_bounds(geometry: &FragmentGeometry, data: Option<&LayoutInputData>, rect: Rect) -> Rect {
    let outline = data
        .filter(|data| data.outline_width > 0.0)
        .map_or(0.0, |data| data.outline_offset + data.outline_width);
    let width = border_width(geometry).max(outline);

    rect + Insets::uniform(width)
}

/// The area a border `width` wide stroked outside the border box `rect`
/// paints in, cut at the sides in `joins`.
fn border_clip(rect: Rect, width: f64, joins: JoinedSides) -> Rect {
    let side = |joined: bool| if joined { 0.0 } else { width };

    rect + Insets::new(
        side(joins.left),
        side(joins.top),
        side(joins.right),
        side(joins.bottom),
    )
}

fn border_width(geometry: &FragmentGeometry) -> f64 {
    let border = geometry.border;

    border
        .left_top
        .x
        .max(border.left_top.y)
        .max(border.right_bottom.x)
        .max(border.right_bottom.y)
}

/// Paint the background, border and outline of a box's border box `rect`.
/// The background and border are rounded with the box's
/// [border radii](FragmentData::border_radii) from `paint_data`, square at
/// the joins of a split box unless it clones its decoration, and the outline with all four corners of its
/// [border radius](FragmentData::border_radius).
fn draw_box(
    sources: Sources<'_>,
    fragment: &BoxFragment,
    data: Option<&LayoutInputData>,
    paint_data: Option<&FragmentData>,
    rect: Rect,
    transform: Affine,
    scene: &mut impl PaintScene,
) {
    let Some(props) = paint_props(sources.ui, sources.inputs, fragment.source) else {
        return;
    };
    let anti_aliasing = anti_aliasing(props);
    let decoration_break = data.map_or_else(BoxDecorationBreak::default, |data| {
        data.box_decoration_break
    });
    let radii = paint_data.map_or(RoundedRectRadii::from(0.0), |data| {
        data.border_radii(fragment, decoration_break)
    });

    if let Some(fill) = props.get::<Background>()
        && let Some(paint) = scene_paint(&fill.0)
    {
        scene.fill(
            peniko::Fill::NonZero,
            transform,
            paint,
            None,
            &rect.to_rounded_rect(radii),
            anti_aliasing,
        );
    }

    let width = border_width(&fragment.layout.geometry);

    if width > 0.0
        && let Some(fill) = props.get::<BorderFill>()
        && let Some(paint) = scene_paint(&fill.0)
    {
        // A broken box leaves out the border on the sides joining its other
        // fragments: the border is stroked outside the box, so clipping at
        // those sides cuts it there, and the sides around run on into the
        // next fragment.
        let joins = match decoration_break {
            BoxDecorationBreak::Break => JoinedSides::of(fragment),
            BoxDecorationBreak::Clone => JoinedSides::default(),
        };
        let clipped = !joins.is_empty();
        if clipped {
            scene.push_clip_layer(
                peniko::Fill::NonZero,
                transform,
                &border_clip(rect, width, joins),
            );
        }

        scene.stroke(
            &Stroke::new(width),
            transform,
            paint,
            None,
            &rect.inset(-width / 2.0).to_rounded_rect(radii),
            anti_aliasing,
        );

        if clipped {
            scene.pop_layer();
        }
    }

    if let Some(data) = data.filter(|data| data.outline_width > 0.0)
        && let Some(fill) = props.get::<OutlineFill>()
        && let Some(paint) = scene_paint(&fill.0)
    {
        let width = data.outline_width;

        scene.stroke(
            &Stroke::new(width),
            transform,
            paint,
            None,
            &outline_path(
                rect,
                paint_data
                    .and_then(|data| data.border_radius)
                    .unwrap_or_default(),
                data.outline_offset,
                width,
            ),
            anti_aliasing,
        );
    }
}

/// The scene paint of `paint`. `None` for
/// [content](Paint::Content) paints, which are not drawn yet.
fn scene_paint(paint: &Paint) -> Option<PaintRef<'_>> {
    match paint {
        Paint::Solid(color) => Some(anyrender::Paint::Solid(*color)),
        Paint::Gradient(gradient) => Some(anyrender::Paint::Gradient(gradient)),
        // TODO: draw the content over the filled shape.
        Paint::Content(_) => None,
    }
}

/// How a node draws the edges of its shapes and glyphs, from its
/// [`AntiAliasing`](style::AntiAliasing) in `props`.
fn anti_aliasing(props: Props<'_>) -> AntiAliasing {
    if props.get_or_default::<style::AntiAliasing>().0 {
        AntiAliasing::Enabled
    } else {
        AntiAliasing::Disabled
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_clips_cut_the_joined_sides() {
        let rect = Rect::new(10.0, 20.0, 50.0, 40.0);

        // Closed boxes clip nowhere inside their border.
        assert_eq!(
            border_clip(rect, 2.0, JoinedSides::default()),
            Rect::new(8.0, 18.0, 52.0, 42.0)
        );

        // The middle piece of an inline element is open on both ends.
        let middle = JoinedSides {
            left: true,
            right: true,
            ..JoinedSides::default()
        };
        assert_eq!(
            border_clip(rect, 2.0, middle),
            Rect::new(10.0, 18.0, 50.0, 42.0)
        );

        // The first fragment of a box split down the page is open below.
        let first = JoinedSides {
            bottom: true,
            ..JoinedSides::default()
        };
        assert_eq!(
            border_clip(rect, 2.0, first),
            Rect::new(8.0, 18.0, 52.0, 40.0)
        );
    }
}
