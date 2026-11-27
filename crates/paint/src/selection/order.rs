//! The selected text of each inline layout, in tree order.

use comet_tree::ui::Ui;
use parley::{Affinity, InlineBoxKind};

use comet_layout::{
    fragment::{BoxFragmentKey, BoxFragmentType, inline::inline_box_source, tree::FragmentTree},
    input::tree::LayoutInputTree,
};

use crate::selection::{Selection, TextPosition, layout_position};

/// A position in the text of an inline layout, as parley takes it.
pub type LayoutEnd = (usize, Affinity);

/// The selected part of the text of one inline layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedRange {
    /// The box owning the inline layout.
    pub owner: BoxFragmentKey,
    pub start: LayoutEnd,
    pub end: LayoutEnd,
    /// Whether the text starts a new block, such as a paragraph, rather
    /// than continuing the text before it on the same line, around an
    /// inline box.
    pub breaks: bool,
}

/// The selected part of each inline layout under `root` that `selection`
/// covers, in tree order. The U+FFFC character of an inline box holding
/// other inline layouts is never selected: its text is selected on its own,
/// so a selection ending in the box stops before or after it in the text
/// around it. Empty if the selection is collapsed or an end is not laid out
/// under `root`.
///
/// `inputs` is the input tree `fragments` were built from, from the UI of
/// `ui`.
pub fn selected_ranges(
    ui: &Ui,
    inputs: &LayoutInputTree,
    fragments: &FragmentTree,
    root: BoxFragmentKey,
    selection: Selection,
) -> SelectedRanges {
    ranges_between(ui, inputs, fragments, root, selection).unwrap_or_default()
}

fn ranges_between(
    ui: &Ui,
    inputs: &LayoutInputTree,
    fragments: &FragmentTree,
    root: BoxFragmentKey,
    selection: Selection,
) -> Option<SelectedRanges> {
    if selection.is_collapsed() {
        return None;
    }
    fragments.get(root)?;
    let end = |position: TextPosition| -> Option<End> {
        let (owner, index) = layout_position(ui, inputs, fragments, position)?;
        Some((owner, index, position.affinity.into()))
    };
    let (anchor, extent) = (end(selection.anchor)?, end(selection.extent)?);

    let order = TextOrder::new(fragments, root);
    let anchor = (order.key(anchor)?, anchor.2);
    let extent = (order.key(extent)?, extent.2);
    let (start, last) = if anchor.0 <= extent.0 {
        (anchor, extent)
    } else {
        (extent, anchor)
    };

    Some(SelectedRanges {
        next: start.0.0,
        start,
        last,
        order,
    })
}

/// The iterator of [`selected_ranges`].
#[derive(Default)]
pub struct SelectedRanges {
    order: TextOrder,
    /// The segment and index of the start of the selection, with its
    /// affinity.
    start: ((usize, usize), Affinity),
    /// The segment and index of the end of the selection, with its
    /// affinity. Without a selection, the order has no segments.
    last: ((usize, usize), Affinity),
    /// The next segment to visit.
    next: usize,
}

impl Iterator for SelectedRanges {
    type Item = SelectedRange;

    fn next(&mut self) -> Option<SelectedRange> {
        let ((start, start_index), start_affinity) = self.start;
        let ((last, last_index), last_affinity) = self.last;

        while self.next <= last {
            let index = self.next;
            self.next += 1;
            let segment = self.order.segments.get(index)?;
            let from = if index == start {
                (start_index, start_affinity)
            } else {
                (segment.start, Affinity::Downstream)
            };
            let to = if index == last {
                (last_index, last_affinity)
            } else {
                (segment.end, Affinity::Upstream)
            };

            if from.0 < to.0 {
                return Some(SelectedRange {
                    owner: segment.owner,
                    start: from,
                    end: to,
                    breaks: segment.breaks,
                });
            }
        }

        None
    }
}

/// An end of the selection in the text of an inline layout: the box owning
/// it, the index, and the affinity.
type End = (BoxFragmentKey, usize, Affinity);

/// A part of the text of an inline layout, between the inline boxes
/// holding other inline layouts: positions `start..=end`, with its owner.
struct Segment {
    owner: BoxFragmentKey,
    start: usize,
    end: usize,
    /// See [`SelectedRange::breaks`].
    breaks: bool,
}

/// The text of the inline layouts under a box, in tree order, split into
/// [`Segment`]s.
///
/// The inline layouts inside an inline box come between the positions
/// before and after its U+FFFC character, so a position is ordered by its
/// segment, then its index. Segments skip those characters.
#[derive(Default)]
struct TextOrder {
    segments: Vec<Segment>,
    /// Whether the next inline layout starts inside an inline box, on the
    /// line of the text before it.
    inline_start: bool,
}

impl TextOrder {
    fn new(fragments: &FragmentTree, root: BoxFragmentKey) -> Self {
        let mut order = Self::default();
        order.visit(fragments, root);
        order
    }

    fn visit(&mut self, fragments: &FragmentTree, fragment: BoxFragmentKey) {
        let Some(inline) = fragments.inline_layout(fragment) else {
            // The run boxes and blocks of a box split by blocks are its
            // children in flow order. Inline part boxes hold no text.
            for child in fragments.boxes.children(fragment) {
                if fragments
                    .get(child)
                    .is_some_and(|child| child.box_type != BoxFragmentType::InlinePart)
                {
                    self.visit(fragments, child);
                }
            }
            return;
        };

        let mut start = 0;
        let mut breaks = !std::mem::take(&mut self.inline_start);
        for inline_box in inline.layout.inline_boxes() {
            // Only boxes in flow have a U+FFFC character: those out of flow
            // are visited after the text.
            if !matches!(inline_box.kind, InlineBoxKind::InFlow) {
                continue;
            }
            let Some(child) = inline
                .atomics
                .get(&inline_box_source(inline_box.id))
                .copied()
                .filter(|&child| {
                    fragments
                        .boxes
                        .descendants(child)
                        .any(|key| fragments.inline_layout(key).is_some())
                })
            else {
                continue;
            };

            let index = inline_box.index;
            self.segments.push(Segment {
                owner: fragment,
                start,
                end: index,
                breaks,
            });
            self.inline_start = true;
            self.visit(fragments, child);
            self.inline_start = false;
            start = index + '\u{FFFC}'.len_utf8();
            breaks = false;
        }
        self.segments.push(Segment {
            owner: fragment,
            start,
            end: inline.text.len(),
            breaks,
        });

        // Boxes placed under the owner rather than on its lines, such as
        // positioned boxes taken out of the flow. Its other children are the
        // atomic inlines visited above and inline part boxes.
        for child in fragments.boxes.children(fragment) {
            if fragments.get(child).is_some_and(|child| {
                matches!(
                    child.box_type,
                    BoxFragmentType::OutOfFlow | BoxFragmentType::Floating
                )
            }) {
                self.visit(fragments, child);
            }
        }
    }

    /// Where `end` is in the order: its segment and index. `None` if its
    /// inline layout is not under the root.
    fn key(&self, (owner, index, _): End) -> Option<(usize, usize)> {
        let segment = self.segments.iter().position(|segment| {
            segment.owner == owner && (segment.start..=segment.end).contains(&index)
        })?;

        Some((segment, index))
    }
}
