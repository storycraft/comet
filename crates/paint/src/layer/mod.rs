//! Paint layers of a fragment tree, deciding the order fragments paint in.
//!
//! A [`PaintLayerTree`](tree::PaintLayerTree) groups the fragments under a
//! root box into layers, like stacking contexts in CSS. Fragments are walked
//! in the paint order tree of [`prepaint::fragment`](crate::prepaint::fragment):
//! boxes hold their child boxes or the items of their inline layouts, and
//! pieces of inline elements hold the items of their subtrees.
//!
//! Layers are owned by box fragments. A principal box whose item sets a
//! transform, an opacity, a filter, a backdrop filter, a mix blend mode or a
//! z index other than auto starts a child layer it owns. So does an inline
//! element setting one of them other than a transform, as inline elements
//! are not transformed: its layer is owned by the first of its linked
//! [inline part](comet_layout::fragment::BoxFragmentType::InlinePart) boxes,
//! and covers every piece of the element, across lines and runs. A layer
//! covers its owner and its descendants, except those starting layers of
//! their own and their descendants.
//!
//! A layer paints its owner, then its child layers with a negative z index,
//! then the rest of its fragments in paint order, then its other child
//! layers. It paints with the transform of its owner, looked up in the
//! [`PrepaintTree`](crate::prepaint::tree::PrepaintTree), and the opacity,
//! filters and blend mode resolved into the layer from its owner's item.

pub mod builder;
pub mod tree;

use std::sync::Arc;

use comet_layout::fragment::BoxFragmentKey;
use peniko::BlendMode;
use slotmap::new_key_type;

new_key_type! {
    pub struct PaintLayerKey;
}

/// A layer of fragments painting together, owned by a fragment.
#[derive(Debug, Clone)]
pub struct PaintLayerItem {
    /// Box starting the layer: a principal box, or the first inline part box
    /// of an inline element. Its pre-paint data gives the transform the layer
    /// paints with.
    pub owner: BoxFragmentKey,

    /// Z index of the layer among its sibling layers, from its owner's
    /// [`ZIndex`](crate::style::ZIndex). An auto z index counts as 0, and
    /// the root layer has z index 0.
    pub z_index: i32,

    /// Opacity the layer blends with, from its owner's
    /// [`Opacity`](crate::style::Opacity), clamped between 0 and 1.
    pub opacity: f32,

    /// Filter of the layer's contents, built from its owner's
    /// [`Filter`](crate::style::Filter) effects, in the owner's local
    /// coordinates. The filter region is the owner's
    /// [overflow](comet_layout::fragment::FragmentGeometry::overflow), joined
    /// with those of the other pieces of an inline element, grown by the
    /// expansion of each effect. `None` if the owner sets no filter.
    pub filter: Option<Arc<anyrender::Filter>>,

    /// Filter of what is painted behind the owner, built from its
    /// [`BackdropFilter`](crate::style::BackdropFilter) effects, in the
    /// owner's local coordinates. The filter region is the owner's border
    /// box, joined with those of the other pieces of an inline element, grown
    /// by the expansion of each effect. `None` if the owner sets
    /// no backdrop filter.
    pub backdrop_filter: Option<Arc<anyrender::Filter>>,

    /// Blend mode the layer is composited with onto what is painted behind
    /// it, from its owner's [`MixBlend`](crate::style::MixBlend). `None` if
    /// the owner sets none.
    pub blend_mode: Option<BlendMode>,

    /// The last build or update that reached the layer, to find the layers
    /// it left behind.
    pub(crate) pass: u32,
}
