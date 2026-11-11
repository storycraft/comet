//! Inline content laid out with Parley.
//!
//! Lines are owned by the box of an inline formatting context, or by an
//! anonymous block when block boxes interrupt the inline content:
//! 1. [`flow`]: split inline content interrupted by block boxes into runs, each
//!    in its own anonymous block.
//! 2. [`shape`]: build the Parley layout of a fragment owning lines.
//! 3. Break it into lines and align them.
//! 4. [`emit`]: replace the line, span and glyph run fragments, and link the
//!    fragments of each source across lines.

mod emit;
mod events;
mod flow;
mod shape;

use rustc_hash::FxHashMap;
use taffy::{ResolveOrZero, style::CoreStyle};

use super::{Engine, FragmentKey, Mode};
use crate::{
    fragment::{InlineLayout, tree::FragmentTree},
    input::{LayoutInputKey, LayoutInputKind},
};

/// Buffers for inline layout, retained between builds.
#[derive(Default)]
pub(super) struct Scratch {
    /// Inline span fragments created during emission, in creation order.
    spans: Vec<FragmentKey>,
    /// Span fragments of the current line by inline item.
    span_parents: FxHashMap<LayoutInputKey, FragmentKey>,
    /// Latest fragment of each source, for linking fragments across lines.
    links: FxHashMap<LayoutInputKey, FragmentKey>,
    /// Inline elements reopened at the start of a run.
    ancestors: Vec<LayoutInputKey>,
    /// Atomic inline children of the inline layout being prepared.
    atomics: Vec<LayoutInputKey>,
}

/// Result of breaking inline content into lines.
struct Flow {
    width: f32,
    height: f32,
}

impl Engine<'_> {
    /// Lay out the lines of an inline formatting context's box, or of an
    /// anonymous block.
    pub(super) fn layout_lines(
        &mut self,
        fragment: FragmentKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let Some(style) = self
            .fragments
            .get_state(fragment)
            .map(|state| state.style.clone())
        else {
            return taffy::LayoutOutput::HIDDEN;
        };
        let padding = style
            .padding()
            .resolve_or_zero(input.parent_size.width, |_, _| 0.0);
        let border = style
            .border()
            .resolve_or_zero(input.parent_size.width, |_, _| 0.0);
        let inset = padding + border;

        let mut output = taffy::compute_leaf_layout(
            input,
            &style,
            |_, _| 0.0,
            |_, available| self.measure_lines(fragment, available),
        );

        if input.run_mode == taffy::RunMode::PerformLayout {
            let width = (output.size.width - inset.left - inset.right).max(0.0);
            let available = taffy::Size {
                width: taffy::AvailableSpace::Definite(width),
                height: input.available_space.height,
            };

            self.prepare(fragment, available);
            self.break_lines(fragment, width);
            self.align_lines(fragment);
            self.emit(
                fragment,
                taffy::Point {
                    x: inset.left,
                    y: inset.top,
                },
            );

            // Fragments of runs are linked once the owner's runs are emitted.
            if self
                .fragments
                .get_state(fragment)
                .is_some_and(|state| state.mode == Mode::Inline)
            {
                self.link_inline(fragment);
            }
        }

        if let Some(inline) = self.fragments.get_inline(fragment) {
            output.baselines = taffy::Baselines {
                first: inline.first_baseline.map(|baseline| inset.top + baseline),
                last: inline.last_baseline.map(|baseline| inset.top + baseline),
            };
        }

        output
    }

    fn measure_lines(
        &mut self,
        fragment: FragmentKey,
        available: taffy::Size<taffy::AvailableSpace>,
    ) -> taffy::Size<f32> {
        self.invalidate_final(fragment);

        self.prepare(fragment, available);

        let flow = match available.width {
            taffy::AvailableSpace::Definite(width) => self.break_lines(fragment, width),
            taffy::AvailableSpace::MaxContent => self.break_lines(fragment, f32::MAX),
            taffy::AvailableSpace::MinContent => {
                // Zero width determines the intrinsic minimum. Measure height at
                // that width so short words can share a line in the final layout.
                let flow = self.break_lines(fragment, 0.0);
                self.break_lines(fragment, flow.width)
            }
        };

        taffy::Size {
            width: flow.width,
            height: flow.height,
        }
    }

    /// Shape the inline content if needed, lay out atomic inline children, and
    /// update their inline box metrics.
    fn prepare(&mut self, fragment: FragmentKey, available: taffy::Size<taffy::AvailableSpace>) {
        if !self
            .fragments
            .get_inline(fragment)
            .is_some_and(|inline| inline.shaped)
        {
            self.shape(fragment);
        }

        let tree = self.tree;
        let Some(inline) = self.fragments.get_inline(fragment) else {
            return;
        };
        let mut atomics = std::mem::take(&mut self.scratch.inline.atomics);
        atomics.clear();
        atomics.extend(
            inline
                .boxes
                .iter()
                .copied()
                .filter(|&key| tree.items[key].kind == LayoutInputKind::AtomicInline),
        );

        for &key in &atomics {
            let atomic = self.ensure_box(key);
            self.compute(atomic, available);
        }

        self.scratch.inline.atomics = atomics;
        self.update_inline_boxes(fragment);
    }

    /// Copy the margin box metrics of atomic inline children into their
    /// inline boxes, so lines can be broken again without reshaping.
    fn update_inline_boxes(&mut self, fragment: FragmentKey) {
        let tree = self.tree;
        // Taken out while reading the states of the atomic inlines.
        let Some(mut inline) = self.fragments.take_inline(fragment) else {
            return;
        };
        let FragmentTree {
            items,
            states,
            first,
            ..
        } = &*self.fragments;
        let InlineLayout { layout, boxes, .. } = &mut *inline;

        for inline_box in layout.inline_boxes_mut() {
            let key = boxes[inline_box.id as usize];

            if tree.items[key].kind != LayoutInputKind::AtomicInline {
                continue;
            }

            let Some(state) = first
                .get(key)
                .and_then(|&atomic| items.get(atomic)?.item.state())
                .and_then(|key| states.get(key))
            else {
                continue;
            };
            let layout = state.layout;

            // Parley places the margin box; fragments retain border-box
            // coordinates and dimensions.
            inline_box.width = layout.size.width + layout.margin.left + layout.margin.right;
            inline_box.height = layout.size.height + layout.margin.top + layout.margin.bottom;
            inline_box.baseline = Some(layout.margin.top + state.baseline);
        }

        if let Some(state) = self.fragments.get_state_mut(fragment) {
            state.inline = Some(inline);
        }
    }

    /// Break the lines at `max_advance`, measuring their extent and baselines.
    fn break_lines(&mut self, fragment: FragmentKey, max_advance: f32) -> Flow {
        let Some(inline) = self.fragments.get_inline_mut(fragment) else {
            return Flow {
                width: 0.0,
                height: 0.0,
            };
        };
        inline.layout.break_all_lines(Some(max_advance));

        let mut width = 0.0f32;
        let mut first_baseline = None;
        let mut last_baseline = None;

        for line in inline.layout.lines() {
            let metrics = line.metrics();
            width = width.max(metrics.advance - metrics.hanging_advance);
            first_baseline.get_or_insert(metrics.baseline);
            last_baseline = Some(metrics.baseline);
        }

        inline.first_baseline = first_baseline;
        inline.last_baseline = last_baseline;

        Flow {
            width,
            height: inline.layout.height(),
        }
    }

    /// Align the lines by the owner's alignment. Alignment only moves content
    /// within lines, so measuring skips it.
    fn align_lines(&mut self, fragment: FragmentKey) {
        let owner = self.fragments.items[fragment].source;
        let align = self.tree.items[owner].text_style.align;

        if let Some(inline) = self.fragments.get_inline_mut(fragment) {
            inline
                .layout
                .align(align, parley::AlignmentOptions::default());
        }
    }
}
