//! How an item's box lays out its content, and how it takes part in the
//! layout of its parent.

use taffy::style::CoreStyle;

use crate::{
    fragment2::{BoxFragmentType, ChildLayoutType},
    input::{ChildLayout, LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
    taffy::style::Style,
};

/// The kind of [`ChildLayoutType`] of a box, without its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChildKind {
    Block,
    Inline,
    Container,
    Content,
}

impl ChildKind {
    /// How the box of `key` lays out its content.
    ///
    /// Text and inline flow content establish an inline formatting context.
    /// A block flow box does too when any child is not block level; its
    /// block level children are then inline part boxes.
    pub fn of_item(tree: &LayoutInputTree, key: LayoutInputKey) -> Self {
        let item = &tree.items[key];

        match item.child_layout {
            ChildLayout::Container => Self::Container,
            ChildLayout::Inline => Self::Inline,
            ChildLayout::None if item.kind == LayoutInputKind::Text => Self::Inline,
            ChildLayout::None => Self::Content,
            ChildLayout::Block
                if tree
                    .items
                    .children(key)
                    .any(|child| tree.items[child].kind != LayoutInputKind::Block) =>
            {
                Self::Inline
            }
            ChildLayout::Block => Self::Block,
        }
    }

    pub fn of(layout: &ChildLayoutType) -> Self {
        match layout {
            ChildLayoutType::Block => Self::Block,
            ChildLayoutType::Inline(_) => Self::Inline,
            ChildLayoutType::Container => Self::Container,
            ChildLayoutType::Content => Self::Content,
        }
    }
}

/// The type of a box that is a child of a block or container box, or a
/// build root, from its style.
pub fn block_box_type(style: &Style) -> BoxFragmentType {
    if is_out_of_flow(style) {
        BoxFragmentType::OutOfFlow
    } else if style.establishes_flow_root() {
        BoxFragmentType::FlowRoot
    } else {
        BoxFragmentType::Normal
    }
}

/// Whether a box with this style is taken out of flow.
pub fn is_out_of_flow(style: &Style) -> bool {
    style.position() == taffy::Position::Absolute
}
