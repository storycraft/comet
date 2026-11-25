use std::ops::Deref;

use comet_tree::{node::NodeKey, style::StyleProp, ui::Props};
use taffy::LayoutPartialTree;

use crate::{
    fragment::state::NodeLayoutBits,
    fragment2::{
        BoxFragmentKey,
        builder::taffy::{BoxTree, node_id},
    },
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
    ///
    /// Also return what the layout depends on beyond the box styles of the
    /// container and its children, such as custom lengths using `vw` or `vh`
    /// ([`NodeLayoutBits::DEPENDS_VIEWPORT`]) or resolving against the
    /// container's parent ([`NodeLayoutBits::DEPENDS_PARENT_SIZE`]). Lengths
    /// resolving against the container itself need no bits.
    fn layout(
        &self,
        context: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> (taffy::LayoutOutput, NodeLayoutBits);
}

/// Restricted access to the current box and its direct children only.
///
/// Every child of a container generates its own box, including text and
/// inline children, which establish their own inline formatting contexts.
/// Child indices follow input tree order; out-of-range indices panic.
pub struct ContainerContext<'e, 'a> {
    /// Taffy tree of the container's box fragment, buffering its child
    /// boxes in input tree order.
    pub(crate) tree: BoxTree<'e, 'a>,
}

impl ContainerContext<'_, '_> {
    /// Box fragment of the container.
    fn root(&self) -> BoxFragmentKey {
        self.tree.key()
    }

    /// Box fragment of a direct child.
    fn child(&self, index: usize) -> BoxFragmentKey {
        self.tree.children()[index]
    }

    /// The input item of a box fragment.
    fn item(&self, key: BoxFragmentKey) -> LayoutInputKey {
        self.tree.fragments().boxes[key].source
    }

    /// The source node of an input item.
    fn item_node(&self, key: LayoutInputKey) -> NodeKey {
        self.tree.input_tree().items[key].node
    }

    /// The source node of this container.
    pub fn node(&self) -> NodeKey {
        self.item_node(self.item(self.root()))
    }

    /// Properties of the container, without inheritance or defaults. Prefer
    /// this over [`Self::prop`] when reading several properties.
    pub fn props(&self) -> Option<Props<'_>> {
        self.tree.ui().props(self.node())
    }

    /// Read a custom property on the container without inheritance or
    /// defaults.
    pub fn prop<T: StyleProp>(&self) -> Option<impl Deref<Target = T> + '_> {
        self.tree.ui().prop::<T>(self.node())
    }

    pub fn style(&self) -> &Style {
        self.tree.fragments().boxes[self.root()].style()
    }

    pub fn child_count(&self) -> usize {
        self.tree.children().len()
    }

    pub fn child_style(&self, index: usize) -> &Style {
        self.tree.fragments().boxes[self.child(index)].style()
    }

    /// The source node of a direct child box.
    pub fn child_node(&self, index: usize) -> NodeKey {
        self.item_node(self.item(self.child(index)))
    }

    /// Read a property on the child's source node. This does not inherit
    /// properties from the container.
    pub fn child_prop<T: StyleProp>(&self, index: usize) -> Option<impl Deref<Target = T> + '_> {
        self.tree.ui().prop::<T>(self.child_node(index))
    }

    /// Properties of the child's source node. Prefer this over
    /// [`Self::child_prop`] when reading several properties.
    pub fn child_props(&self, index: usize) -> Option<Props<'_>> {
        self.tree.ui().props(self.child_node(index))
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
        let mut constraint = self.tree.constraint().clone();
        constraint.units.font_size =
            f64::from(self.tree.input_tree().items[key].text_style.span.font_size);

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
        let id = node_id(self.child(index));
        self.tree.compute_child_layout(id, input)
    }

    /// Set the direct child's box, including padding, border and margin metrics.
    /// Measure it with matching known dimensions first so its contents fit.
    pub fn set_child_layout(&mut self, index: usize, layout: taffy::Layout) {
        let id = node_id(self.child(index));
        self.tree.set_unrounded_layout(id, &layout);
    }
}
