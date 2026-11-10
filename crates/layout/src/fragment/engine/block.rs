//! Block level layout with Taffy: roots, leaves, custom containers and
//! baselines. Block containers themselves use `taffy::compute_block_layout`
//! through the adapter.

use taffy::style::CoreStyle;

use super::{Engine, to_taffy_key};
use crate::{
    container::ContainerContext,
    input::{ChildLayout, LayoutInputKey, LayoutInputKind},
    style::bx::DisplayInner,
};

impl Engine<'_> {
    /// Lay out a box as an independent root of the available space.
    pub fn compute(&mut self, key: LayoutInputKey, available: taffy::Size<taffy::AvailableSpace>) {
        taffy::compute_root_layout(self, to_taffy_key(key), available);

        if key == self.root || self.tree.items[key].kind == LayoutInputKind::AtomicInline {
            let baseline = self.inline_baseline(key);
            let state = &mut self.fragments.states[key];
            state.baseline = baseline;
            self.fragments.items[state.fragment].layout.baseline = Some(baseline as f64);
        }
    }

    /// Lay out a box without children, sized by its style alone.
    pub(super) fn layout_leaf(
        &mut self,
        key: LayoutInputKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let style = self.fragments.states[key].style.clone();

        taffy::compute_leaf_layout(input, &style, |_, _| 0.0, |_, _| taffy::Size::ZERO)
    }

    /// Lay out a container with its custom algorithm. Containers without an
    /// algorithm are empty leaves.
    pub(super) fn layout_container(
        &mut self,
        key: LayoutInputKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let ui = self.ui;
        let inner = ui.prop::<DisplayInner>(self.tree.items[key].node);

        let Some(DisplayInner::Container(algorithm)) = inner.as_deref() else {
            return self.layout_leaf(key, input);
        };

        let children = self.tree.items.children(key).collect();
        let mut context = ContainerContext {
            engine: self,
            root: key,
            children,
        };

        algorithm.layout(&mut context, input)
    }

    /// Last in-flow line baseline, measured from the border-box top.
    fn last_baseline(&self, key: LayoutInputKey) -> Option<f32> {
        let bx = self.fragments.states[key].layout;

        if let Some(inline) = self.fragments.inline_layouts.get(key) {
            return inline
                .last_baseline
                .map(|baseline| bx.border.top + bx.padding.top + baseline);
        }

        if self.tree.items[key].child_layout != ChildLayout::Block {
            return None;
        }

        self.tree.items.children(key).rev().find_map(|child| {
            let state = self.fragments.states.get(child)?;

            if state.style.position() == taffy::Position::Absolute {
                return None;
            }

            self.last_baseline(child)
                .map(|baseline| state.layout.location.y + baseline)
        })
    }

    /// Baseline relative to the border-box top. The fallback is the bottom
    /// margin edge, which can lie outside the border box.
    fn inline_baseline(&self, key: LayoutInputKey) -> f32 {
        let state = &self.fragments.states[key];

        if state.style.overflow().x == taffy::Overflow::Visible
            && state.style.overflow().y == taffy::Overflow::Visible
            && let Some(baseline) = self.last_baseline(key)
        {
            return baseline;
        }

        state.layout.size.height + state.layout.margin.bottom
    }
}
