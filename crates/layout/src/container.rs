use std::ops::Deref;

use comet_tree::{node::NodeKey, style::StyleProp};
use taffy::LayoutPartialTree;

use crate::{
    taffy::{TaffyLayout, style::Style, to_taffy_key},
    tree::LayoutItemKey,
};

/// A custom layout algorithm for one box and its direct children.
pub trait ContainerLayout: Send + Sync {
    /// Return this container's content size in `LayoutOutput::size`. Other
    /// output metadata is ignored by the enclosing leaf layout. Measure
    /// children through `context`, then assign their border-box sizes and
    /// positions relative to this container's content box. Descendant layout
    /// is handled by the engine when a child is measured with its final known
    /// dimensions.
    ///
    /// Honor `input.run_mode`: measurement calls need not place children.
    /// Taffy applies this container's sizing constraints, padding and border.
    /// Known dimensions and available space refer to its content area; do not
    /// subtract its padding or border again.
    fn layout(
        &self,
        context: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput;
}

/// Restricted access to the current box and its direct children only.
/// Child indices follow layout-tree order; out-of-range indices panic.
pub struct ContainerContext<'a, 'ui> {
    pub(crate) engine: &'a mut TaffyLayout<'ui>,
    pub(crate) root: LayoutItemKey,
    pub(crate) children: Vec<LayoutItemKey>,
    pub(crate) content_offset: taffy::Point<f32>,
}

impl ContainerContext<'_, '_> {
    /// The source node of this container.
    pub fn node(&self) -> Option<NodeKey> {
        self.engine.tree.items[self.root].ty.node()
    }

    /// Read a custom property on the container without inheritance or defaults.
    pub fn prop<T: StyleProp>(&self) -> Option<impl Deref<Target = T> + '_> {
        self.engine.ui.prop::<T>(self.node()?)
    }

    pub fn style(&self) -> &Style {
        &self.engine.nodes[&self.root].style
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    pub fn child_style(&self, index: usize) -> &Style {
        &self.engine.nodes[&self.children[index]].style
    }

    /// The source node represented by a direct child box. For an anonymous
    /// wrapper, this is the wrapped inline element or text, including when
    /// block interruptions split that element into several fragments.
    pub fn child_node(&self, index: usize) -> Option<NodeKey> {
        self.engine
            .tree
            .items
            .descendants(self.children[index])
            .find_map(|key| self.engine.tree.items[key].ty.node())
    }

    /// Read a property on the child's source node, including wrapped children.
    /// This does not inherit properties from the container.
    pub fn child_prop<T: StyleProp>(&self, index: usize) -> Option<impl Deref<Target = T> + '_> {
        self.engine.ui.prop::<T>(self.child_node(index)?)
    }

    /// Lay out a child's contents without exposing its descendants for mutation.
    pub fn measure_child(
        &mut self,
        index: usize,
        mut input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        input.vertical_margins_are_collapsible = taffy::Line::FALSE;

        self.engine
            .compute_child_layout(to_taffy_key(self.children[index]), input)
    }

    /// Set the direct child's box, including padding, border and margin metrics.
    /// Measure it with matching known dimensions first so its contents fit.
    pub fn set_child_layout(&mut self, index: usize, mut layout: taffy::Layout) {
        layout.location.x += self.content_offset.x;
        layout.location.y += self.content_offset.y;

        self.engine
            .nodes
            .get_mut(&self.children[index])
            .unwrap()
            .layout = layout;
    }
}
