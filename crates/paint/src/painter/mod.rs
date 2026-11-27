//! Painting of fragments into a scene, in the order of their paint layers.
//!
//! A [`CometCompositePainter`] walks the [paint layers](crate::layer) of a
//! fragment tree. It places each layer with its transform and the scroll
//! offsets of its ancestors, filters what is behind it with its backdrop
//! filter, groups it with its opacity, filter and blend mode, and clips it
//! with the boxes clipping it, then paints its child layers in z order.
//!
//! The fragments of each layer are drawn by a [`CometLayerPainter`]: the
//! owner's background, border and outline, and its descendants painting in
//! the layer, without its child layers. It draws the contents of a single
//! layer, so it can also draw layers on their own, such as into compositor
//! surfaces.

pub mod composite;
pub mod layer;

pub use composite::CometCompositePainter;
pub use layer::CometLayerPainter;

use anyrender::PaintScene;
use comet_layout::{
    fragment::{BoxFragment, BoxFragmentKey, FragmentGeometry, FragmentRef, tree::FragmentTree},
    input::tree::LayoutInputTree,
};
use comet_tree::ui::Ui;
use kurbo::{Affine, Point, Rect, Vec2};

use crate::{
    layer::tree::{PaintLayerTree, layer_owner},
    prepaint::{fragment::BoxRole, tree::PrepaintTree},
    selection::rects::SelectionRects,
};

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
