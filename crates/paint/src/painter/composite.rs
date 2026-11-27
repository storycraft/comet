//! Painting of paint layers into a scene, in z order.

use std::sync::Arc;

use anyrender::PaintScene;
use comet_content::manager::ContentManager;
use comet_layout::{
    fragment::{BoxFragment, BoxFragmentKey, tree::FragmentTree},
    input::tree::LayoutInputTree,
};
use comet_tree::ui::Ui;
use kurbo::{Affine, Rect};
use peniko::BlendMode;

use super::{Bases, CometLayerPainter, Sources, Transforms, local_rect, push_clip};
use crate::{
    layer::{PaintLayerKey, tree::PaintLayerTree},
    prepaint::{fragment::BoxRole, tree::PrepaintTree},
    selection::rects::SelectionRects,
};

/// Paints final fragments in the order of their [`PaintLayerTree`], with the
/// [`PrepaintTree`] resolved for them. Each layer is placed, filtered,
/// grouped and clipped here, and its fragments are drawn with a
/// [`CometLayerPainter`]. Keep it to reuse its buffers across paints.
#[derive(Default)]
pub struct CometCompositePainter {
    /// Draws the fragments of each layer.
    layer: CometLayerPainter,
    /// Ancestors of a child layer's owner, up to a box of its parent layer
    /// with known transforms, innermost first.
    chain: Vec<BoxFragmentKey>,
}

impl CometCompositePainter {
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
    /// A [content](crate::style::Paint::Content) background draws its
    /// content from `contents` over the border box, clipped to it and rounded
    /// with its border radius. A box displaying a content, with
    /// `DisplayInner::Content`, draws it over its content box, clipped to it,
    /// after its background and border and before its outline. Contents
    /// missing from `contents` draw nothing.
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
        contents: &ContentManager,
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
            contents,
            layers,
            viewport,
            scale_factor,
        };

        self.layer.clear();
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
        self.layer
            .draw_owner(sources, owner, fragment, transforms, &bases, clip, scene);
        let (clip, clipped) = if inline {
            (clip, false)
        } else {
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

        self.layer
            .draw_contents(sources, owner, fragment, transforms, &bases, clip, scene);

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
