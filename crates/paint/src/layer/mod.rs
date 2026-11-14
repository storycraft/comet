//! Paint layers of a fragment tree, deciding the order fragments paint in.
//!
//! A [`PaintLayerTree`](tree::PaintLayerTree) groups the fragments under a
//! root fragment into layers, like stacking contexts in CSS. A box fragment
//! whose item sets a [`Transform`](crate::style::Transform), an
//! [`Opacity`](crate::style::Opacity) or a [`ZIndex`](crate::style::ZIndex)
//! starts a child layer it owns. Every other fragment paints in the layer of
//! its nearest owning ancestor. A layer paints with the transform and opacity
//! of its owner fragment, looked up in the
//! [`PrePaintTree`](crate::pre_paint::tree::PrePaintTree).

pub mod builder;
pub mod tree;
