//! Common types for styling nodes and building input and fragment trees.

pub use crate::container::{ContainerContext, ContainerLayout};
pub use crate::fragment::{
    Fragment, FragmentBox, FragmentItem, FragmentKey, builder::FragmentTreeBuilder,
    tree::FragmentTree,
};
pub use crate::input::{
    ChildLayout, LayoutFlags, LayoutInputItem, LayoutInputKey, LayoutInputKind,
    builder::{BuildRequest, LayoutInputTreeBuilder},
    tree::LayoutInputTree,
};
pub use crate::style::{
    self, LayoutConstraint,
    bx::*,
    geometry::{self, AutoValue, Point, Rect, StyleUnit},
};
