//! Paint layers of a fragment tree, deciding the order fragments paint in.
//!
//! A [`PaintLayerTree`](tree::PaintLayerTree) groups the fragments under a
//! root fragment into layers, like stacking contexts in CSS. A box fragment
//! whose item sets a transform, an opacity, a filter, a backdrop filter or a
//! z index other than auto starts a child layer it owns. A layer covers its owner fragment and its
//! descendants, except those starting layers of their own and their
//! descendants.
//!
//! A layer paints its owner, then its child layers with a negative z index,
//! then the rest of its fragments in tree order, then its other child layers.
//! It paints with the transform of its owner fragment, looked up in the
//! [`PrepaintTree`](crate::prepaint::tree::PrepaintTree), and the opacity and
//! filters resolved into the layer from its owner's item.

pub mod builder;
pub mod tree;

use std::sync::Arc;

use comet_layout::fragment::FragmentKey;
use slotmap::new_key_type;

new_key_type! {
    pub struct PaintLayerKey;
}

/// A layer of fragments painting together, owned by a fragment.
#[derive(Debug, Clone)]
pub struct PaintLayerItem {
    /// Fragment starting the layer. Its pre-paint data gives the transform the
    /// layer paints with.
    pub owner: FragmentKey,

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
    /// [overflow](comet_layout::fragment::FragmentBox::overflow), grown by the
    /// expansion of each effect. `None` if the owner sets no filter.
    pub filter: Option<Arc<anyrender::Filter>>,

    /// Filter of what is painted behind the owner, built from its
    /// [`BackdropFilter`](crate::style::BackdropFilter) effects, in the
    /// owner's local coordinates. The filter region is the owner's border
    /// box, grown by the expansion of each effect. `None` if the owner sets
    /// no backdrop filter.
    pub backdrop_filter: Option<Arc<anyrender::Filter>>,

    /// The last build or update that reached the layer, to find the layers
    /// it left behind.
    pub(crate) pass: u32,
}
