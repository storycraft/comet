//! Extracting the selected text.

use comet_layout::{fragment::inline::InlineLayout, input::LayoutInputKey};

use super::{
    Window,
    text_order::{SelectedRange, SelectedRanges},
};
use crate::{selection::Selection, style::Selectable};

impl Window {
    /// The text `selection` covers, from the last layout, as chunks in tree
    /// order: concatenated, they are the selected text. See
    /// [`SelectedText`].
    ///
    /// ```ignore
    /// let text: String = window.selected_text(selection).collect();
    /// ```
    pub fn selected_text(&self, selection: Selection) -> SelectedText<'_> {
        SelectedText {
            window: self,
            ranges: self.selected_ranges(selection),
            current: None,
            pending_break: false,
            emitted: false,
        }
    }
}

/// The selected text of a [`Window`], as chunks in tree order, from
/// [`Window::selected_text`].
///
/// The text is as laid out, after white space processing. Each chunk is
/// the selected part of one text node, or a `"\n"` between the text of
/// different blocks, such as paragraphs. Text that cannot be
/// [selected](Selectable) and inline boxes without text, such as images,
/// are left out. Empty if the selection is collapsed or an end is not laid
/// out.
pub struct SelectedText<'a> {
    window: &'a Window,
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
        let inline = self
            .window
            .pipeline
            .trees
            .fragments
            .inline_layout(range.owner);
        self.current = inline.map(|inline| (inline, range.start.0, range.end.0));
        self.pending_break = range.breaks && self.emitted;
    }

    /// The first text item in `inline` ending after `from` and starting
    /// before `to`, by its start, and its range of the text.
    fn next_item(
        inline: &InlineLayout,
        from: usize,
        to: usize,
    ) -> Option<(LayoutInputKey, core::ops::Range<usize>)> {
        if from >= to {
            return None;
        }

        // Inline elements cover the ranges of their content: only text
        // items are read.
        inline
            .offset_mapping
            .iter()
            .filter(|entry| entry.text)
            .map(|entry| (entry.source, entry.range.clone()))
            .filter(|(_, range)| range.end > from && range.start < to && !range.is_empty())
            .min_by_key(|(_, range)| range.start)
    }

    /// Whether the text of the layout item `item` can be selected: see
    /// [`Selectable`].
    fn selectable(&self, item: LayoutInputKey) -> bool {
        let node = self.window.pipeline.trees.node(item);
        node.is_some()
            && self
                .window
                .inherited::<Selectable, _>(node, |prop| prop.0)
                .unwrap_or(true)
    }
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
            let Some((item, range)) = Self::next_item(inline, from, to) else {
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
