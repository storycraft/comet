//! Paint layers of a fragment tree, deciding the order fragments paint in.
//!
//! A [`PaintLayerTree`](tree::PaintLayerTree) groups the fragments under a
//! root fragment into layers, like stacking contexts in CSS. A box fragment
//! with a [stacking](crate::prepaint::FragmentData::stacking) z index, such
//! as one whose item sets a transform, an opacity, a filter or a z index,
//! starts a child layer it owns. A layer covers its owner fragment and its
//! descendants, except those starting layers of their own and their
//! descendants.
//!
//! A layer paints its owner, then its child layers with a negative z index,
//! then the rest of its fragments in tree order, then its other child layers.
//! It paints with the transform of its owner fragment, looked up in the
//! [`PrepaintTree`](crate::prepaint::tree::PrepaintTree), and the opacity and
//! filters of its owner's item.

pub mod builder;
pub mod tree;

use comet_layout::fragment::FragmentKey;
use slotmap::new_key_type;

new_key_type! {
    pub struct PaintLayerKey;
}

/// A layer of fragments painting together, owned by a fragment.
#[derive(Debug, Clone)]
pub struct PaintLayerItem {
    /// Fragment starting the layer. Its pre-paint data gives the transform the
    /// layer paints with, and its item's style the opacity and filters.
    pub owner: FragmentKey,

    /// Z index of the layer among its sibling layers. The root layer has z
    /// index 0.
    pub z_index: i32,

    /// The last build or update that reached the layer, to find the layers
    /// it left behind.
    pub(crate) pass: u32,
}
