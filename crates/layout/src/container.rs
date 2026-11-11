use std::ops::Deref;

use comet_tree::{node::NodeKey, style::StyleProp};
use taffy::LayoutPartialTree;

use crate::{
    fragment::{FragmentKey, adapter::FragmentEngine, engine::node_id},
    input::LayoutInputKey,
    style::geometry::StyleUnit,
    taffy::{conv, style::Style},
};

/// A custom layout algorithm for one box and its direct children.
pub trait ContainerLayout: Send + Sync {
    /// Lay out this container using the original Taffy input and return its
    /// border-box size and layout metadata. The algorithm owns its sizing
    /// constraints, padding and border, available through `context.style()`.
    ///
    /// Measure children through `context`, then assign their border-box sizes
    /// and positions relative to this container's border box. Descendant layout
    /// is handled by the engine when a child is measured with its final known
    /// dimensions.
    fn layout(
        &self,
        context: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput;
}

/// Restricted access to the current box and its direct children only.
///
/// Every child of a container generates its own box, including text and
/// inline children, which establish their own inline formatting contexts.
/// Child indices follow input tree order; out-of-range indices panic.
pub struct ContainerContext<'a, 'ui> {
    /// Engine scoped to the container's box fragment and its child boxes, in
    /// input tree order.
    pub(crate) engine: FragmentEngine<'a, 'ui>,
}

impl ContainerContext<'_, '_> {
    /// Box fragment of the container.
    fn root(&self) -> FragmentKey {
        self.engine.fragment()
    }

    /// Box fragment of a direct child.
    fn child(&self, index: usize) -> FragmentKey {
        self.engine.children()[index]
    }

    /// The source node of this container.
    pub fn node(&self) -> NodeKey {
        self.engine.tree.items[self.item(self.root())].node
    }

    /// Read a custom property on the container without inheritance or defaults.
    pub fn prop<T: StyleProp>(&self) -> Option<impl Deref<Target = T> + '_> {
        self.engine.ui.prop::<T>(self.node())
    }

    pub fn style(&self) -> &Style {
        &self.engine.fragments.states[self.root()].style
    }

    /// The input item of a box fragment.
    fn item(&self, fragment: FragmentKey) -> LayoutInputKey {
        self.engine.fragments.items[fragment].source
    }

    pub fn child_count(&self) -> usize {
        self.engine.children().len()
    }

    pub fn child_style(&self, index: usize) -> &Style {
        &self.engine.fragments.states[self.child(index)].style
    }

    /// The source node of a direct child box.
    pub fn child_node(&self, index: usize) -> NodeKey {
        self.engine.tree.items[self.item(self.child(index))].node
    }

    /// Read a property on the child's source node. This does not inherit
    /// properties from the container.
    pub fn child_prop<T: StyleProp>(&self, index: usize) -> Option<impl Deref<Target = T> + '_> {
        self.engine.ui.prop::<T>(self.child_node(index))
    }

    /// Resolve font and viewport units for the container, keeping percentages
    /// symbolic so the layout algorithm can select their reference size.
    pub fn resolve_length(&self, value: StyleUnit) -> taffy::LengthPercentage {
        self.resolve_item_length(self.item(self.root()), value)
    }

    /// Resolve an item's custom length using its computed font size.
    pub fn resolve_child_length(&self, index: usize, value: StyleUnit) -> taffy::LengthPercentage {
        self.resolve_item_length(self.item(self.child(index)), value)
    }

    fn resolve_item_length(
        &self,
        key: LayoutInputKey,
        value: StyleUnit,
    ) -> taffy::LengthPercentage {
        let mut constraint = self.engine.constraint.clone();
        constraint.units.font_size =
            f64::from(self.engine.tree.items[key].text_style.span.font_size);

        conv::length(value, &constraint)
    }

    /// Lay out a child's contents without exposing its descendants for mutation.
    pub fn measure_child(
        &mut self,
        index: usize,
        mut input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        input.vertical_margins_are_collapsible = taffy::Line::FALSE;

        // Block children position the out-of-flow boxes they contain while
        // laying out, since their IDs belong to Comet's tree rather than the
        // custom algorithm's local adapter.
        let key = node_id(self.child(index));
        self.engine.compute_child_layout(key, input)
    }

    /// Set the direct child's box, including padding, border and margin metrics.
    /// Measure it with matching known dimensions first so its contents fit.
    pub fn set_child_layout(&mut self, index: usize, layout: taffy::Layout) {
        let key = node_id(self.child(index));
        self.engine.set_unrounded_layout(key, &layout);
    }
}
