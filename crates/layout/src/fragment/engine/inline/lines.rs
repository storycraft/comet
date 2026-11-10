//! Line breaking, with block children placed between lines.

use super::{super::Engine, Flow, from_box_id};
use crate::{fragment::tree::FragmentTree, input::LayoutInputKey};

/// Content placed by the line breaker, in flow order.
pub(super) enum Placed {
    /// Index of a line in the Parley layout.
    Line(usize),
    /// Block box interrupting the inline content, with the top of its margin box
    /// relative to the content box.
    Block { key: LayoutInputKey, top: f32 },
}

/// Running state of one line breaking pass.
struct Lines<'a> {
    /// Placed content, recorded only when emitting.
    events: Option<&'a mut Vec<Placed>>,
    /// Index of the next line in the Parley layout.
    index: usize,
    width: f32,
    first_baseline: Option<f32>,
    last_baseline: Option<f32>,
    /// The current line only holds block boxes and the line break after them.
    block_line: bool,
}

impl Lines<'_> {
    fn place(&mut self, placed: Placed) {
        if let Some(events) = &mut self.events {
            events.push(placed);
        }
    }

    /// Record a committed line. Returns the line top to rewind to when the
    /// line only holds blocks; the blocks already advanced the flow.
    fn line(&mut self, data: &parley::LineBreakData) -> Option<f64> {
        let index = self.index;
        self.index += 1;

        if std::mem::take(&mut self.block_line) {
            return Some(data.line_y_start);
        }

        let metrics = data.metrics;
        self.width = self.width.max(metrics.advance - metrics.hanging_advance);
        self.first_baseline.get_or_insert(metrics.baseline);
        self.last_baseline = Some(metrics.baseline);
        self.place(Placed::Line(index));

        None
    }

    /// Place a block box at `top`. Returns the top of the content after it.
    fn block(&mut self, key: LayoutInputKey, layout: &taffy::Layout, top: f64) -> f64 {
        let margin = layout.margin;

        self.width = self
            .width
            .max(margin.left + layout.size.width + margin.right);
        self.place(Placed::Block {
            key,
            top: top as f32,
        });
        self.block_line = true;

        top + (margin.top + layout.size.height + margin.bottom) as f64
    }
}

impl Engine<'_> {
    /// Break the inline content into lines of `max_advance`, placing block
    /// children between them. With `emit`, placed content is recorded for
    /// [`Self::emit_inline`].
    pub(super) fn break_inline(
        &mut self,
        key: LayoutInputKey,
        max_advance: f32,
        emit: bool,
    ) -> Flow {
        let FragmentTree {
            inline_layouts,
            states,
            ..
        } = &mut *self.fragments;
        let inline = &mut inline_layouts[key];
        let events = &mut self.scratch.inline.events;
        events.clear();

        let mut lines = Lines {
            events: emit.then_some(events),
            index: 0,
            width: 0.0,
            first_baseline: None,
            last_baseline: None,
            block_line: false,
        };

        let mut breaker = inline.layout.break_lines();
        breaker.state_mut().set_layout_max_advance(max_advance);
        // Parley 0.12 breaks against the line max advance only. It is never
        // derived from the layout max advance and defaults to zero, so both are set.
        breaker.state_mut().set_line_max_advance(max_advance);

        while let Some(data) = breaker.break_next() {
            match data {
                parley::YieldData::LineBreak(data) => {
                    if let Some(top) = lines.line(&data) {
                        breaker.state_mut().set_line_y(top);
                    }
                }
                parley::YieldData::InlineBoxBreak(data) => {
                    let child = from_box_id(data.inline_box_id);
                    let state = breaker.state_mut();
                    let bottom = lines.block(child, &states[child].layout, state.line_y());

                    state.set_line_y(bottom);
                    // Out-of-flow extents keep the box from contributing to the line.
                    state.append_inline_box_to_line(
                        data.advance,
                        f32::NEG_INFINITY,
                        f32::NEG_INFINITY,
                    );
                }
                // No maximum line height is set.
                parley::YieldData::MaxHeightExceeded(_) => {}
            }
        }

        let height = breaker.committed_y() as f32;
        breaker.finish();

        inline.first_baseline = lines.first_baseline;
        inline.last_baseline = lines.last_baseline;

        Flow {
            width: lines.width,
            height,
        }
    }
}
