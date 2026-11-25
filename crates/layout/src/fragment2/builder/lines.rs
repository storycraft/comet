//! Laying out an inline formatting context: the boxes inside it, then its
//! lines.
//!
//! The inline layout is shaped when it is built, so laying it out never
//! shapes again:
//! 1. Lay out the in-flow boxes inside the inline content as Taffy children
//!    of the owner, and write their margin boxes into their Parley inline
//!    boxes.
//! 2. Break the lines at the available width, and align them.
//! 3. [Emit](super::emit) the inline items, placing the boxes inside.
//! 4. Hand the out-of-flow boxes inside to Taffy: the owner lays out those
//!    it is the containing block of, and returns the others in its
//!    [`taffy::LayoutOutput`] for an ancestor to lay out.

use parley::Alignment;
use taffy::{LayoutPartialTree, ResolveOrZero, style::CoreStyle};

use super::{
    Engine,
    adapter::{BoxTree, node_id, scrollable_overflow},
};
use crate::fragment2::{BoxFragmentKey, BoxFragmentType, ChildLayoutType, inline_box_source};

/// Byte length of the character standing for an in-flow inline box.
const OBJECT_REPLACEMENT_LEN: usize = '\u{FFFC}'.len_utf8();

/// Extent of broken lines.
struct Flow {
    width: f32,
    height: f32,
}

impl Engine<'_> {
    /// Lay out the box `key` establishing an inline formatting context, as a
    /// Taffy leaf measured by its lines.
    pub(super) fn layout_inline(
        &mut self,
        key: BoxFragmentKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let style = self.fragments.boxes[key].style.clone();
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
            |_, available| self.measure_lines(key, available),
        );

        if input.run_mode == taffy::RunMode::PerformLayout {
            let width = (output.size.width - inset.left - inset.right).max(0.0);
            let available = taffy::Size {
                width: taffy::AvailableSpace::Definite(width),
                height: input.available_space.height,
            };

            self.prepare(key, available);
            let align = self.alignment(key);
            self.break_lines(key, width, align);

            let origin = kurbo::Vec2::new(f64::from(inset.left), f64::from(inset.top));

            // Taffy only knows the measured lines. Boxes inside and
            // overflowing lines can reach further.
            if let Some(content) = self.emit(key, origin) {
                let content = scrollable_overflow(content, border);
                output.scrollable_overflow_rect = union(output.scrollable_overflow_rect, content);
            }

            // Out-of-flow boxes inside, and those left by the boxes inside,
            // in document order.
            output.oof_candidates = std::mem::take(&mut self.scratch.candidates);
            output.oof_positioning_area = Some(taffy::OofPositioningArea {
                size: taffy::Size {
                    width: (output.size.width - border.left - border.right).max(0.0),
                    height: (output.size.height - border.top - border.bottom).max(0.0),
                },
                offset: taffy::Point {
                    x: border.left,
                    y: border.top,
                },
            });
            taffy::compute_oof_layout(
                &mut BoxTree::root_pass(self, key),
                node_id(key),
                &mut output,
            );
            self.place_hoisted(key);
        }

        if let Some(inline) = self.fragments.inline_layout(key) {
            output.baselines = taffy::Baselines {
                first: inline.first_baseline.map(|baseline| inset.top + baseline),
                last: inline.last_baseline.map(|baseline| inset.top + baseline),
            };
        }

        output
    }

    /// The content size of the lines in the available space.
    fn measure_lines(
        &mut self,
        key: BoxFragmentKey,
        available: taffy::Size<taffy::AvailableSpace>,
    ) -> taffy::Size<f32> {
        self.invalidate_final(key);
        self.prepare(key, available);

        let flow = match available.width {
            taffy::AvailableSpace::Definite(width) => {
                self.break_lines(key, width, Alignment::Start)
            }
            taffy::AvailableSpace::MaxContent => self.break_lines(key, f32::MAX, Alignment::Start),
            taffy::AvailableSpace::MinContent => {
                // Zero width determines the intrinsic minimum. The height is
                // measured at that width, so short words can share a line.
                let flow = self.break_lines(key, 0.0, Alignment::Start);
                self.break_lines(key, flow.width, Alignment::Start)
            }
        };

        taffy::Size {
            width: flow.width,
            height: flow.height,
        }
    }

    /// The in-flow boxes inside the inline content of `key`, in flow order.
    fn collect_in_flow_boxes(&mut self, key: BoxFragmentKey) {
        let mut children = std::mem::take(&mut self.scratch.inline_children);
        children.clear();

        if let Some(inline) = self.fragments.inline_layout(key) {
            children.extend(
                inline
                    .layout
                    .inline_boxes()
                    .filter_map(|inline_box| {
                        inline
                            .atomics
                            .get(&inline_box_source(inline_box.id))
                            .copied()
                    })
                    .filter(|&child| {
                        self.fragments
                            .get(child)
                            .is_some_and(|fragment| fragment.box_type != BoxFragmentType::OutOfFlow)
                    }),
            );
        }

        self.scratch.inline_children = children;
    }

    /// Lay out the in-flow boxes inside the inline content of `key`, and
    /// write their margin boxes into their inline boxes.
    ///
    /// Inline part boxes take the available width, atomic inlines their
    /// size within it.
    fn prepare(&mut self, key: BoxFragmentKey, available: taffy::Size<taffy::AvailableSpace>) {
        self.collect_in_flow_boxes(key);
        let children = std::mem::take(&mut self.scratch.inline_children);

        for &child in &children {
            self.layout_inline_child(key, child, available);
        }

        self.scratch.inline_children = children;
        self.update_inline_boxes(key);
    }

    /// Lay out the in-flow box `child` inside the inline content of `owner`
    /// as a Taffy child of the owner, in the available space of its content
    /// box. Its size, margins and baseline are recorded for its inline box,
    /// and the out-of-flow candidates it leaves for placing it.
    ///
    /// An inline part box with an auto width stretches to the available
    /// width. Atomic inlines size themselves.
    fn layout_inline_child(
        &mut self,
        owner: BoxFragmentKey,
        child: BoxFragmentKey,
        available: taffy::Size<taffy::AvailableSpace>,
    ) {
        let parent_width = available.width.into_option();
        let fragment = &self.fragments.boxes[child];
        let style = &fragment.style;
        let margin = style.margin().resolve_or_zero(parent_width, |_, _| 0.0);
        let padding = style.padding().resolve_or_zero(parent_width, |_, _| 0.0);
        let border = style.border().resolve_or_zero(parent_width, |_, _| 0.0);
        let known_width = (fragment.box_type == BoxFragmentType::InlinePart
            && style.size().width.is_auto())
        .then(|| parent_width.map(|width| (width - margin.left - margin.right).max(0.0)))
        .flatten();
        let previous = fragment.layout.unrounded;

        let output = BoxTree::root_pass(self, owner).compute_child_layout(
            node_id(child),
            taffy::LayoutInput {
                known_dimensions: taffy::Size {
                    width: known_width,
                    height: None,
                },
                known_dimensions_are_definite: taffy::Size {
                    width: true,
                    height: true,
                },
                parent_size: taffy::Size {
                    width: parent_width,
                    height: None,
                },
                available_space: available,
                sizing_mode: taffy::SizingMode::InherentSize,
                axis: taffy::RequestedAxis::Both,
                run_mode: taffy::RunMode::PerformLayout,
                vertical_margins_are_collapsible: taffy::Line::FALSE,
            },
        );

        // Placed by the lines when emitted.
        let layout = taffy::Layout {
            order: previous.order,
            location: previous.location,
            size: output.size,
            scrollable_overflow_rect: output.scrollable_overflow_rect,
            scrollbar_size: taffy::Size::ZERO,
            border,
            padding,
            margin,
        };
        self.set_unrounded_layout(child, &layout);

        let baseline = self.inline_baseline(child);
        self.fragments.boxes[child].layout.baseline = baseline;
        self.scratch
            .child_candidates
            .insert(child, output.oof_candidates);
    }

    /// Copy the margin boxes of the in-flow boxes inside the inline content
    /// of `key` into their inline boxes, so lines can be broken again
    /// without shaping.
    ///
    /// The margins between inline part boxes on consecutive lines collapse:
    /// the top margin of the later box shrinks by the overlap, recorded for
    /// placing the box.
    fn update_inline_boxes(&mut self, key: BoxFragmentKey) {
        let Some(fragment) = self.fragments.get_mut(key) else {
            return;
        };
        // Taken out while reading the boxes inside.
        let mut inline =
            match std::mem::replace(&mut fragment.child_layout, ChildLayoutType::Content) {
                ChildLayoutType::Inline(inline) => inline,
                other => {
                    fragment.child_layout = other;
                    return;
                }
            };

        let collapse = &mut self.scratch.collapse;
        collapse.clear();
        // End of the previous inline part box in the text, with its bottom
        // margin.
        let mut previous_part: Option<(usize, f32)> = None;

        let count = inline.layout.inline_boxes().len();
        for index in 0..count {
            let Some((id, text_index)) = inline
                .layout
                .inline_boxes()
                .nth(index)
                .map(|inline_box| (inline_box.id, inline_box.index))
            else {
                continue;
            };
            let source = inline_box_source(id);
            let Some(child) = inline
                .atomics
                .get(&source)
                .copied()
                .and_then(|child| self.fragments.get(child))
            else {
                previous_part = None;
                continue;
            };

            if child.box_type == BoxFragmentType::OutOfFlow {
                continue;
            }

            let layout = &child.layout.unrounded;
            let width = layout.size.width + layout.margin.left + layout.margin.right;
            let mut height = layout.size.height + layout.margin.top + layout.margin.bottom;
            let mut baseline =
                layout.margin.top + child.layout.baseline.unwrap_or(layout.size.height);

            if child.box_type == BoxFragmentType::InlinePart {
                let overlap = previous_part
                    .filter(|&(end, _)| {
                        inline
                            .text
                            .get(end..text_index)
                            .is_some_and(|between| between.bytes().all(|byte| byte == b'\n'))
                    })
                    .map_or(0.0, |(_, bottom)| {
                        bottom.max(0.0).min(layout.margin.top.max(0.0))
                    });

                if overlap > 0.0 {
                    height -= overlap;
                    baseline -= overlap;
                    collapse.insert(source, overlap);
                }

                previous_part = Some((text_index + OBJECT_REPLACEMENT_LEN, layout.margin.bottom));
            } else {
                previous_part = None;
            }

            inline.layout.edit_inline_box(index, |inline_box| {
                inline_box.width = width;
                inline_box.height = height;
                inline_box.baseline = Some(baseline);
            });
        }

        self.fragments.boxes[key].child_layout = ChildLayoutType::Inline(inline);
    }

    /// Break the lines of `key` at `max_advance` with `align`, recording
    /// their baselines. Alignment only moves content within lines, so
    /// measuring passes [`Alignment::Start`].
    fn break_lines(&mut self, key: BoxFragmentKey, max_advance: f32, align: Alignment) -> Flow {
        let Some(inline) = self
            .fragments
            .get_mut(key)
            .and_then(|fragment| fragment.inline_mut())
        else {
            return Flow {
                width: 0.0,
                height: 0.0,
            };
        };
        inline
            .layout
            .set_alignment(align, parley::AlignmentOptions::default());
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

    /// The alignment of the lines, from the text style of the owner.
    fn alignment(&self, key: BoxFragmentKey) -> Alignment {
        let source = self.fragments.boxes[key].source;
        self.cx.tree.items[source].text_style.align
    }
}

/// The smallest rectangle containing `a` and `b`.
fn union(a: taffy::Rect<f32>, b: taffy::Rect<f32>) -> taffy::Rect<f32> {
    taffy::Rect {
        left: a.left.min(b.left),
        right: a.right.max(b.right),
        top: a.top.min(b.top),
        bottom: a.bottom.max(b.bottom),
    }
}
