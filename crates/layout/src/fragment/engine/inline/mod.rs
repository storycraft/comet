//! Inline formatting contexts laid out with Parley.
//!
//! An inline layout runs in three steps:
//! 1. [`shape`]: build the Parley layout from the inline content. Block children
//!    become custom out-of-flow boxes surrounded by line breaks.
//! 2. [`lines`]: break it into lines, placing block children between them.
//! 3. [`emit`]: replace the line, span and glyph run fragments.

mod emit;
mod events;
mod lines;
mod shape;

use rustc_hash::FxHashMap;
use slotmap::KeyData;
use taffy::{ResolveOrZero, style::CoreStyle};

use self::{
    events::{InlineEvents, Op},
    lines::Placed,
};
use super::Engine;
use crate::{
    fragment::{FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, LayoutInputKind},
};

/// Buffers for inline layout, retained between builds.
#[derive(Default)]
pub(super) struct Scratch {
    /// Content placed by the latest emitting line break.
    events: Vec<Placed>,
    /// Inline span fragments created during emission, in creation order.
    spans: Vec<FragmentKey>,
    /// Span fragments of the current line by inline item.
    span_parents: FxHashMap<LayoutInputKey, FragmentKey>,
    /// Latest fragment of each source, for linking fragments across lines.
    links: FxHashMap<LayoutInputKey, FragmentKey>,
}

fn from_box_id(id: u64) -> LayoutInputKey {
    KeyData::from_ffi(id).into()
}

/// Result of breaking an inline formatting context into lines.
struct Flow {
    width: f32,
    height: f32,
}

impl Engine<'_> {
    pub(super) fn layout_inline(
        &mut self,
        key: LayoutInputKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let style = self.fragments.states[key].style.clone();
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
            |known, available| self.measure_inline(key, known, available),
        );

        if input.run_mode == taffy::RunMode::PerformLayout {
            let width = (output.size.width - inset.left - inset.right).max(0.0);
            let available = taffy::Size {
                width: taffy::AvailableSpace::Definite(width),
                height: input.available_space.height,
            };

            self.prepare_inline(key, available);
            self.break_inline(key, width, true);
            self.emit_inline(
                key,
                taffy::Point {
                    x: inset.left,
                    y: inset.top,
                },
            );
        }

        if let Some(inline) = self.fragments.inline_layouts.get(key) {
            output.baselines = taffy::Baselines {
                first: inline.first_baseline.map(|baseline| inset.top + baseline),
                last: inline.last_baseline.map(|baseline| inset.top + baseline),
            };
        }

        output
    }

    fn measure_inline(
        &mut self,
        key: LayoutInputKey,
        known: taffy::Size<Option<f32>>,
        available: taffy::Size<taffy::AvailableSpace>,
    ) -> taffy::Size<f32> {
        self.invalidate_final(key);

        let available = taffy::Size {
            width: known
                .width
                .map(taffy::AvailableSpace::Definite)
                .unwrap_or(available.width),
            height: available.height,
        };

        self.prepare_inline(key, available);

        let flow = match available.width {
            taffy::AvailableSpace::Definite(width) => self.break_inline(key, width, false),
            taffy::AvailableSpace::MaxContent => self.break_inline(key, f32::MAX, false),
            taffy::AvailableSpace::MinContent => {
                // Zero width determines the intrinsic minimum. Measure height at
                // that width so short words can share a line in the final layout.
                let flow = self.break_inline(key, 0.0, false);
                self.break_inline(key, flow.width, false)
            }
        };

        taffy::Size {
            width: flow.width,
            height: flow.height,
        }
    }

    /// Shape the inline content if needed, lay out atomic inline and block
    /// children, and update the inline box metrics.
    fn prepare_inline(
        &mut self,
        key: LayoutInputKey,
        available: taffy::Size<taffy::AvailableSpace>,
    ) {
        if !self
            .fragments
            .inline_layouts
            .get(key)
            .is_some_and(|inline| inline.shaped)
        {
            self.shape(key);
        }

        let tree = self.tree;

        for op in InlineEvents::new(tree, key) {
            if let Op::Box(child) | Op::Block(child) = op {
                self.ensure_state(child);
                self.compute(child, available);
            }
        }

        self.update_inline_boxes(key);
    }

    /// Copy the margin box metrics of atomic inline children into their
    /// inline boxes, so lines can be broken again without reshaping.
    fn update_inline_boxes(&mut self, key: LayoutInputKey) {
        let tree = self.tree;
        let FragmentTree {
            inline_layouts,
            states,
            ..
        } = &mut *self.fragments;

        for inline_box in inline_layouts[key].layout.inline_boxes_mut() {
            if inline_box.kind != parley::InlineBoxKind::InFlow {
                continue;
            }

            let child = from_box_id(inline_box.id);

            if tree.items[child].kind != LayoutInputKind::AtomicInline {
                continue;
            }

            let state = &states[child];
            let layout = state.layout;

            // Parley places the margin box; fragments retain border-box
            // coordinates and dimensions.
            inline_box.width = layout.size.width + layout.margin.left + layout.margin.right;
            inline_box.height = layout.size.height + layout.margin.top + layout.margin.bottom;
            inline_box.baseline = Some(layout.margin.top + state.baseline);
        }
    }
}
