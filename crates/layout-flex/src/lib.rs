//! Flexbox containers for Comet.
//!
//! Attach `DisplayInner::Container(flex())` to a node, then set
//! properties from [`styles`] on the container and its children.
//!
//! ```
//! use comet_layout::style::{bx::DisplayInner, geometry::{Size, StyleUnit}};
//! use comet_layout_flex::prelude::*;
//! use comet_tree::{node::Node, ui::Ui};
//!
//! let mut ui = Ui::new();
//! let row = ui.create(Node::Box, (
//!     DisplayInner::Container(flex()),
//!     Gap(Size::new(StyleUnit::Px(12.0), StyleUnit::ZERO)),
//! ));
//! let child = ui.create(Node::Box, (FlexGrow(1.0),));
//! ui.append(row, child);
//! ```

mod adapter;

pub mod prelude;
pub mod styles;

use std::sync::{Arc, LazyLock};

use comet_layout::{
    container::{ContainerContext, ContainerLayout},
    fragment::state::NodeLayoutBits,
};

/// The flexbox algorithm, shared by every flex container.
static FLEX: LazyLock<Arc<dyn ContainerLayout>> = LazyLock::new(|| Arc::new(Flex));

/// The flexbox container algorithm, for [`DisplayInner::Container`].
/// Configure each container and its children with the properties in
/// [`styles`]. Every call returns the same shared instance.
///
/// [`DisplayInner::Container`]: comet_layout::style::bx::DisplayInner::Container
pub fn flex() -> Arc<dyn ContainerLayout> {
    FLEX.clone()
}

/// A flexbox algorithm with configuration supplied by node style properties.
struct Flex;

impl ContainerLayout for Flex {
    fn layout(
        &self,
        context: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> (taffy::LayoutOutput, NodeLayoutBits) {
        adapter::layout(context, input)
    }
}
