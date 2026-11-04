//! Flexbox containers for Comet.
//!
//! Attach `DisplayInner::Container(Box::new(Flex))` to a node, then set
//! properties from [`styles`] on the container and its children.
//!
//! ```
//! use comet_layout::style::{bx::DisplayInner, geometry::{Size, StyleUnit}};
//! use comet_layout_flex::{Flex, styles::{FlexGrow, Gap}};
//! use comet_tree::{node::Node, ui::Ui};
//!
//! let mut ui = Ui::new();
//! let row = ui.create(Node::Box, (
//!     DisplayInner::Container(Box::new(Flex)),
//!     Gap(Size::new(StyleUnit::Px(12.0), StyleUnit::ZERO)),
//! ));
//! let child = ui.create(Node::Box, (FlexGrow(1.0),));
//! ui.append(row, child);
//! ```

mod adapter;

pub mod styles;

use comet_layout::container::{ContainerContext, ContainerLayout};

/// A flexbox algorithm with configuration supplied by node style properties.
#[derive(Debug, Default, Clone, Copy)]
pub struct Flex;

impl ContainerLayout for Flex {
    fn layout(
        &self,
        context: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        adapter::layout(context, input)
    }
}
