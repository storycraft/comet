use std::ops::Deref;

use comet_tree::{node::NodeKey, style::StyleProp};
use taffy::LayoutPartialTree;

use crate::{
    style::geometry::StyleUnit,
    taffy::{TaffyLayout, style::Style, to_taffy_key},
    tree::LayoutItemKey,
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
    /// Honor `input.run_mode`: measurement calls need not place children.
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
        &self.engine.nodes[self.root].style
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }

    pub fn child_style(&self, index: usize) -> &Style {
        &self.engine.nodes[self.children[index]].style
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

    /// Resolve font and viewport units for the container, keeping percentages
    /// symbolic so the layout algorithm can select their reference size.
    pub fn resolve_length(&self, value: StyleUnit) -> taffy::LengthPercentage {
        self.resolve_node_length(self.node(), value)
    }

    /// Resolve an item's custom length using its source node's computed font.
    pub fn resolve_child_length(&self, index: usize, value: StyleUnit) -> taffy::LengthPercentage {
        self.resolve_node_length(self.child_node(index), value)
    }

    fn resolve_node_length(
        &self,
        node: Option<NodeKey>,
        value: StyleUnit,
    ) -> taffy::LengthPercentage {
        let mut constraint = self.engine.constraint.clone();

        if let Some(style) = node.and_then(|node| self.engine.fragments.text_styles.get(&node)) {
            constraint.font_size = f64::from(style.font_size);
        }

        crate::taffy::conv::length(value, &constraint)
    }

    /// Lay out a child's contents without exposing its descendants for mutation.
    pub fn measure_child(
        &mut self,
        index: usize,
        mut input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        input.vertical_margins_are_collapsible = taffy::Line::FALSE;

        let key = to_taffy_key(self.children[index]);
        let mut output = self.engine.compute_child_layout(key, input);

        // Descendant IDs belong to Comet's tree, not the custom algorithm's
        // local adapter. Resolve their positioning before returning the child.
        taffy::compute_oof_layout(self.engine, key, &mut output);

        output
    }

    /// Set the direct child's box, including padding, border and margin metrics.
    /// Measure it with matching known dimensions first so its contents fit.
    pub fn set_child_layout(&mut self, index: usize, layout: taffy::Layout) {
        self.engine
            .nodes
            .get_mut(self.children[index])
            .unwrap()
            .layout = layout;
    }
}
