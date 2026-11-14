//! Paint layers of a fragment tree, deciding the order fragments paint in.
//!
//! A [`PaintLayerTree`](tree::PaintLayerTree) groups the fragments under a
//! root fragment into layers, like stacking contexts in CSS. A box fragment
//! whose item sets a [`Transform`](crate::style::Transform), an
//! [`Opacity`](crate::style::Opacity) or a [`ZIndex`](crate::style::ZIndex)
//! starts a child layer it owns. Every other fragment paints in the layer of
//! its nearest owning ancestor. A layer paints with the transform of its
//! owner fragment, looked up in the
//! [`PrePaintTree`](crate::pre_paint::tree::PrePaintTree), and the opacity of
//! its owner's item.

pub mod builder;
pub mod tree;

use comet_layout::fragment::FragmentKey;
use slotmap::new_key_type;

new_key_type! {
    pub struct PaintLayerKey;
}

/// One entry painted by a layer, in paint order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintLayerEntry {
    /// A fragment painting in the layer, without its descendants: they have
    /// entries of their own.
    Fragment(FragmentKey),
    /// A child layer, painted whole.
    Layer(PaintLayerKey),
}

/// A layer of fragments painting together, owned by a fragment.
#[derive(Debug, Clone)]
pub struct PaintLayerItem {
    /// Fragment starting the layer. Its pre-paint data gives the transform the
    /// layer paints with, and its item's style the opacity.
    pub owner: FragmentKey,

    /// Z index of the layer among the entries of its parent layer.
    pub z_index: i32,

    /// Fragments painting in the layer and its child layers, in paint order.
    ///
    /// The owner fragment comes first. The rest are sorted by z index:
    /// fragments sort at 0, child layers at their own z index, and entries
    /// with equal z index keep their fragment tree order.
    pub entries: Vec<PaintLayerEntry>,
}
