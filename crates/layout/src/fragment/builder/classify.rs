//! How an item's box lays out its content, and how it takes part in the
//! layout of its parent.

use taffy::style::CoreStyle;

use crate::{
    fragment::{
        BoxFragmentType, ChildLayoutType,
        inline::collect::{InlineBuildContext, has_block_in_flow},
    },
    input::{ChildLayout, LayoutInputKey, LayoutInputKind},
    style::bx::Position,
    taffy::style::Style,
};

/// The kind of [`ChildLayoutType`] of a box, without its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChildKind {
    Block,
    Inline,
    MixedInline,
    Container,
    Content,
}

impl ChildKind {
    /// How the box of `key` lays out its content.
    ///
    /// Text and inline flow content establish an inline formatting context.
    /// A block flow box does too when any child is not block level. Inline
    /// content holding a block level box in flow, even inside inline
    /// elements, is mixed: split into runs around its block level boxes.
    pub fn of_item(cx: &InlineBuildContext<'_>, key: LayoutInputKey) -> Self {
        let tree = cx.tree;
        let item = &tree.items[key];

        let inline = match item.child_layout {
            ChildLayout::Container => return Self::Container,
            ChildLayout::None if item.kind == LayoutInputKind::Text => return Self::Inline,
            ChildLayout::None => return Self::Content,
            ChildLayout::Inline => true,
            ChildLayout::Block => tree
                .items
                .children(key)
                .any(|child| tree.items[child].kind != LayoutInputKind::Block),
        };

        match inline {
            false => Self::Block,
            true if has_block_in_flow(cx, key) => Self::MixedInline,
            true => Self::Inline,
        }
    }

    pub fn of(layout: &ChildLayoutType) -> Self {
        match layout {
            ChildLayoutType::Block => Self::Block,
            ChildLayoutType::Inline(_) => Self::Inline,
            ChildLayoutType::MixedInline(_) => Self::MixedInline,
            ChildLayoutType::Container => Self::Container,
            ChildLayoutType::Content => Self::Content,
        }
    }
}

/// The type of the box of `key`, a child of a block or container box, or a
/// build root, with the resolved `style`, or `requested` if given. The box of
/// a text item is always [anonymous](BoxFragmentType::Anonymous): text
/// generates no box of its own, so the box only wraps it in an inline
/// formatting context and paints nothing.
pub fn block_box_type(
    cx: &InlineBuildContext<'_>,
    key: LayoutInputKey,
    style: &Style,
    requested: Option<BoxFragmentType>,
) -> BoxFragmentType {
    if cx.tree.items[key].kind == LayoutInputKind::Text {
        BoxFragmentType::Anonymous
    } else if let Some(box_type) = requested {
        box_type
    } else if is_out_of_flow(style) {
        BoxFragmentType::OutOfFlow
    } else if style.establishes_flow_root() {
        BoxFragmentType::FlowRoot
    } else {
        BoxFragmentType::Normal
    }
}

/// Whether a box with this style is taken out of flow.
pub fn is_out_of_flow(style: &Style) -> bool {
    Position(style.position()).is_out_of_flow()
}

impl ChildKind {
    /// An empty content layout of this kind. Inline layouts start unbuilt.
    pub fn layout(self) -> ChildLayoutType {
        match self {
            Self::Block => ChildLayoutType::Block,
            Self::Inline => ChildLayoutType::Inline(Box::default()),
            Self::MixedInline => ChildLayoutType::MixedInline(None),
            Self::Container => ChildLayoutType::Container,
            Self::Content => ChildLayoutType::Content,
        }
    }
}
