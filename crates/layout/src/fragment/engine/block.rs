//! Block level layout with Taffy: roots, leaves, custom containers and
//! baselines. Block containers use `taffy::compute_block_layout` through the
//! adapter.

use taffy::style::CoreStyle;

use super::{Engine, FragmentEngine, FragmentKey, Mode, fragment_of, node_id};
use crate::{
    container::ContainerContext,
    fragment::tree::FragmentTree,
    input::LayoutInputKind,
    style::{bx::DisplayInner, geometry::Point},
};

impl Engine<'_> {
    /// Lay out a box as an independent root of the available space.
    pub fn compute(
        &mut self,
        fragment: FragmentKey,
        available: taffy::Size<taffy::AvailableSpace>,
    ) {
        taffy::compute_root_layout(
            &mut FragmentEngine::new(self, fragment),
            node_id(fragment),
            available,
        );
        // The root positioning pass places the remaining out-of-flow boxes
        // again, even when the root layout is cached.
        self.place_hoisted(fragment);

        let source = self.fragments.items[fragment].source;

        if source == self.root || self.tree.items[source].kind == LayoutInputKind::AtomicInline {
            let baseline = self.inline_baseline(fragment);

            self.fragments.states[fragment].baseline = baseline;
            self.fragments.items[fragment].layout.baseline = Some(baseline as f64);
        }
    }

    /// Lay out a block container with Taffy's block algorithm, then position
    /// the out-of-flow boxes it contains.
    ///
    /// Taffy bubbles out-of-flow boxes up to their containing block and leaves
    /// their positioning to the caller. Boxes no block claims bubble up to the
    /// root, or to the container laying out this box.
    pub(super) fn layout_block(
        &mut self,
        fragment: FragmentKey,
        input: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        let id = node_id(fragment);
        let mut engine = FragmentEngine::new(self, fragment);
        let mut output = taffy::compute_block_layout(&mut engine, id, input, block_ctx);

        if input.run_mode == taffy::RunMode::PerformLayout {
            taffy::compute_oof_layout(&mut engine, id, &mut output);
            drop(engine);
            self.place_hoisted(fragment);
        }

        output
    }

    /// Make the out-of-flow boxes positioned by `containing` relative to their
    /// parent fragments again.
    ///
    /// Taffy places them relative to their containing block, which can be an
    /// ancestor of their parent when the boxes in between are not positioned.
    /// Their retained Taffy layouts keep containing block coordinates, so this
    /// can run again after every positioning pass.
    fn place_hoisted(&mut self, containing: FragmentKey) {
        let FragmentTree { items, states, .. } = &mut *self.fragments;

        for &id in &states[containing].hoisted {
            let hoisted = fragment_of(id);
            let (mut x, mut y) = (0.0, 0.0);
            let mut ancestor = items.parent(hoisted);

            while let Some(key) = ancestor
                && key != containing
            {
                let location = items[key].layout.location;
                x += location.x;
                y += location.y;
                ancestor = items.parent(key);
            }

            // Not attached below its containing block.
            if ancestor.is_none() {
                continue;
            }

            let location = states[hoisted].layout.location;
            items[hoisted].layout.location =
                Point::new(f64::from(location.x) - x, f64::from(location.y) - y);
        }
    }

    /// Lay out a box without children, sized by its style alone.
    pub(super) fn layout_leaf(
        &mut self,
        fragment: FragmentKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let style = self.fragments.states[fragment].style.clone();

        taffy::compute_leaf_layout(input, &style, |_, _| 0.0, |_, _| taffy::Size::ZERO)
    }

    /// Lay out a container with its custom algorithm. Containers without an
    /// algorithm are empty leaves.
    pub(super) fn layout_container(
        &mut self,
        fragment: FragmentKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let ui = self.ui;
        let source = self.fragments.items[fragment].source;
        let inner = ui.prop::<DisplayInner>(self.tree.items[source].node);

        let Some(DisplayInner::Container(algorithm)) = inner.as_deref() else {
            return self.layout_leaf(fragment, input);
        };

        let mut context = ContainerContext {
            engine: FragmentEngine::new(self, fragment),
        };

        algorithm.layout(&mut context, input)
    }

    /// Last in-flow line baseline, measured from the border-box top.
    fn last_baseline(&self, fragment: FragmentKey) -> Option<f32> {
        let state = &self.fragments.states[fragment];
        let bx = state.layout;

        if let Some(inline) = self.fragments.inline_layouts.get(fragment) {
            return inline
                .last_baseline
                .map(|baseline| bx.border.top + bx.padding.top + baseline);
        }

        if !matches!(state.mode, Mode::Block | Mode::MixedInline) {
            return None;
        }

        self.fragments
            .items
            .children(fragment)
            .rev()
            .find_map(|child| {
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
    fn inline_baseline(&self, fragment: FragmentKey) -> f32 {
        let state = &self.fragments.states[fragment];

        if state.style.overflow().x == taffy::Overflow::Visible
            && state.style.overflow().y == taffy::Overflow::Visible
            && let Some(baseline) = self.last_baseline(fragment)
        {
            return baseline;
        }

        state.layout.size.height + state.layout.margin.bottom
    }
}
