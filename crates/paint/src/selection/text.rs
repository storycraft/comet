//! Extracting the selected text.

use core::ops::Range;

use comet_tree::ui::Ui;

use comet_layout::{
    fragment::{BoxFragmentKey, inline::InlineLayout, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
};

use crate::{
    prepaint::tree::PrepaintTree,
    selection::{
        Selection,
        order::{SelectedRange, SelectedRanges, selected_ranges},
    },
};

/// The text `selection` covers under `root`, as chunks in tree order:
/// concatenated, they are the selected text. See [`SelectedText`].
/// `prepaint` is resolved for `fragments`, and tells which text can be
/// selected.
///
/// ```ignore
/// let text: String =
///     selected_text(ui, inputs, fragments, prepaint, root, selection).collect();
/// ```
pub fn selected_text<'a>(
    ui: &Ui,
    inputs: &LayoutInputTree,
    fragments: &'a FragmentTree,
    prepaint: &'a PrepaintTree,
    root: BoxFragmentKey,
    selection: Selection,
) -> SelectedText<'a> {
    SelectedText {
        prepaint,
        fragments,
        ranges: selected_ranges(ui, inputs, fragments, root, selection),
        current: None,
        pending_break: false,
        emitted: false,
    }
}

/// The selected text under a box, as chunks in tree order, from
/// [`selected_text`].
///
/// The text is as laid out, after white space processing. Each chunk is
/// the selected part of one text node, or a `"\n"` between the text of
/// different blocks, such as paragraphs. Text that cannot be
/// [selected](crate::prepaint::LayoutInputData::selectable), or was not
/// resolved, and inline boxes without text, such as images, are left out.
/// Empty if the selection is collapsed or an end is not laid out.
pub struct SelectedText<'a> {
    prepaint: &'a PrepaintTree,
    fragments: &'a FragmentTree,
    ranges: SelectedRanges,
    /// The inline layout of the range being read, with the index read up
    /// to and the end of the range.
    current: Option<(&'a InlineLayout, usize, usize)>,
    /// Whether a line break comes before the next text of the range being
    /// read.
    pending_break: bool,
    /// Whether any text was emitted.
    emitted: bool,
}

impl<'a> SelectedText<'a> {
    /// Start reading `range`.
    fn enter(&mut self, range: SelectedRange) {
        let inline = self.fragments.inline_layout(range.owner);
        self.current = inline.map(|inline| (inline, range.start.0, range.end.0));
        self.pending_break = range.breaks && self.emitted;
    }

    /// Whether the text of the layout item `item` can be selected.
    fn selectable(&self, item: LayoutInputKey) -> bool {
        self.prepaint.is_selectable(item)
    }
}

/// The first text item in `inline` ending after `from` and starting before
/// `to`, by its start, and its range of the text.
pub fn next_text_item(
    inline: &InlineLayout,
    from: usize,
    to: usize,
) -> Option<(LayoutInputKey, Range<usize>)> {
    if from >= to {
        return None;
    }

    // Inline elements cover the ranges of their content: only text items
    // are read.
    inline
        .offset_mapping
        .iter()
        .filter(|entry| entry.text)
        .map(|entry| (entry.source, entry.range.clone()))
        .filter(|(_, range)| range.end > from && range.start < to && !range.is_empty())
        .min_by_key(|(_, range)| range.start)
}

impl<'a> Iterator for SelectedText<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        loop {
            let Some((inline, from, to)) = self.current else {
                let range = self.ranges.next()?;
                self.enter(range);
                continue;
            };
            let Some((item, range)) = next_text_item(inline, from, to) else {
                self.current = None;
                continue;
            };
            let chunk = range.start.max(from)..range.end.min(to);
            if !self.selectable(item) {
                self.current = Some((inline, chunk.end, to));
                continue;
            }

            // The break goes before the first text of the range, which is
            // read again next.
            if std::mem::take(&mut self.pending_break) {
                return Some("\n");
            }

            self.current = Some((inline, chunk.end, to));
            if let Some(text) = inline.text.get(chunk) {
                self.emitted = true;
                return Some(text);
            }
        }
    }
}
