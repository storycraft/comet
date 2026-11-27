use comet_layout::fragment::{BoxFragmentKey, FragmentRef, tree::FragmentTree};
use comet_slotmap_tree::SlotTree;
use kurbo::{Affine, Vec2};
use slotmap::SecondaryMap;

use crate::{
    layer::{PaintLayerItem, PaintLayerKey},
    prepaint::fragment::{BoxRole, paint_parent, part_of_piece},
};

/// Paint layers of the fragments under a root box, as a tree of layers.
///
/// The child layers of each layer are kept in paint order: sorted by z index,
/// with equal z indices in paint order. Layers only refer to their owner
/// boxes. The fragments painting in a layer are found by walking its owner's
/// descendants in paint order, or those of the pieces of its inline element,
/// skipping the owners of child layers, so changes that keep the set of
/// owners leave the tree as it is.
///
/// Built and updated by a
/// [`PaintLayerBuilder`](super::builder::PaintLayerBuilder), which returns
/// the root layer owned by the root box, also found with [`Self::layer_of`].
/// Layers keep their keys across builds for as long as their owner starts
/// one. The inline part boxes owning the layers of inline elements are
/// created again whenever their inline layout is, and so are their layers.
#[derive(Debug, Default)]
pub struct PaintLayerTree {
    /// The layers, each a child of the layer it paints in. Children are kept
    /// in paint order, and the root layer has no parent.
    ///
    /// Kept in step with the layer of each owner, which
    /// [`Self::layer_of`] looks up. Changes made outside the builder, such as
    /// deleting a layer or changing its owner, must keep them in step.
    pub layers: SlotTree<PaintLayerKey, PaintLayerItem>,
    /// The layer each owner box starts.
    pub(super) by_owner: SecondaryMap<BoxFragmentKey, PaintLayerKey>,
}

impl PaintLayerTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// The layer a box starts, or `None` if it starts none.
    pub fn layer_of(&self, owner: BoxFragmentKey) -> Option<PaintLayerKey> {
        self.by_owner.get(owner).copied()
    }

    /// The layer a fragment paints in: the layer started by the
    /// [layer owner](layer_owner) of its nearest [paint parent](paint_parent)
    /// or self, or `None` if none starts one.
    pub fn enclosing(
        &self,
        fragments: &FragmentTree,
        fragment: FragmentRef,
    ) -> Option<PaintLayerKey> {
        let mut next = Some(fragment);

        while let Some(fragment) = next {
            if let Some(layer) = layer_owner(fragments, fragment).and_then(|key| self.layer_of(key))
            {
                return Some(layer);
            }

            next = paint_parent(fragments, fragment);
        }

        None
    }

    /// Maps the local coordinates of the box `key` in `fragments`, with its
    /// border box's top-left corner at the origin, to the local coordinates
    /// of its parent box: the [own transform](PaintLayerItem::transform) of
    /// the layer it owns, if any, then its layout location:
    /// `translate(location) * own`. Ancestor transforms are not included:
    /// multiply them from the root down to map into scene coordinates.
    /// `None` if `key` is not in `fragments`.
    pub fn transform(&self, fragments: &FragmentTree, key: BoxFragmentKey) -> Option<Affine> {
        let location = Affine::translate(location(fragments, key)?);

        Some(match self.own(key) {
            Some(item) if item.transform != Affine::IDENTITY => location * item.transform,
            _ => location,
        })
    }

    /// Inverse of [`Self::transform`], mapping the local coordinates of the
    /// parent box back to those of the box `key`. `None` if `key` is not in
    /// `fragments`, or its transform collapses an axis.
    pub fn inverse_transform(
        &self,
        fragments: &FragmentTree,
        key: BoxFragmentKey,
    ) -> Option<Affine> {
        let location = Affine::translate(-location(fragments, key)?);

        match self.own(key) {
            Some(item) if item.transform != Affine::IDENTITY => {
                Some(item.inverse_transform? * location)
            }
            _ => Some(location),
        }
    }

    /// The layer the box `owner` owns, if any.
    fn own(&self, owner: BoxFragmentKey) -> Option<&PaintLayerItem> {
        self.layers.get(self.layer_of(owner)?)
    }

    /// The scroll offset a box applies to its descendants: the
    /// [scroll](PaintLayerItem::scroll) of the layer it owns, zero if it owns
    /// none.
    pub fn scroll_offset(&self, owner: BoxFragmentKey) -> Vec2 {
        self.layer_of(owner)
            .and_then(|layer| self.layers.get(layer))
            .map_or(Vec2::ZERO, |item| item.scroll)
    }

    pub fn clear(&mut self) {
        self.layers.clear();
        self.by_owner.clear();
    }
}

/// The layout location of the box `key` in its parent box.
fn location(fragments: &FragmentTree, key: BoxFragmentKey) -> Option<Vec2> {
    let location = fragments.get(key)?.layout.geometry.location;

    Some(Vec2::new(location.x, location.y))
}

/// The box that would own the layer a fragment starts: a principal box
/// itself, or the first linked inline part box of the inline element a piece
/// or inline part box belongs to. `None` for fragments that start no layer,
/// such as anonymous boxes and glyph runs.
pub fn layer_owner(fragments: &FragmentTree, fragment: FragmentRef) -> Option<BoxFragmentKey> {
    let key = match fragment {
        FragmentRef::Box(key) => key,
        FragmentRef::Inline { .. } => part_of_piece(fragments, fragment)?,
    };

    match BoxRole::of(fragments.get(key)?) {
        BoxRole::Principal => Some(key),
        BoxRole::InlinePart => fragments.split_boxes(key).next(),
        BoxRole::Anonymous => None,
    }
}
