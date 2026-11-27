//! Common types for styling nodes and building input and fragment trees.

pub use crate::container::{ContainerContext, ContainerLayout};
pub use crate::dirty_map::{DirtyEntry, DirtyFlags, DirtyMap};
pub use crate::fragment::{
    BoxFragment, BoxFragmentKey, BoxFragmentType, ChildLayoutType, FragmentGeometry, FragmentRef,
    NodeLayoutBits, builder::FragmentTreeBuilder, inline::InlineLayout, tree::FragmentTree,
};
pub use crate::input::{
    ChildLayout, LayoutInputItem, LayoutInputKey, LayoutInputKind,
    builder::{BuildRequest, LayoutInputTreeBuilder},
    tree::LayoutInputTree,
};
pub use crate::style::{self, LayoutConstraint, bx::*};
pub use comet_geometry as geometry;
pub use comet_geometry::{AutoValue, Point, Rect, StyleUnit};
