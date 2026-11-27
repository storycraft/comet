//! The rectangles painted behind the selected text, per text item.

use core::ops::Range;

use comet_layout::{
    fragment::{BoxFragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
};
use comet_tree::ui::Ui;
use kurbo::Rect;
use slotmap::SparseSecondaryMap;

use crate::{
    prepaint::tree::PrepaintTree,
    selection::{Selection, order::SelectedRanges, text::next_text_item},
};

/// The selection of one text item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintSelectionItem {
    /// Indices of the item's rectangles in [`SelectionRects`].
    pub rects: Range<usize>,
}

/// The rectangles behind the selected text of each text item, built from a
/// selection with [`Self::build`].
///
/// Rectangles are in the coordinates of the inline layout holding their
/// item, those of each item stored next to each other in one vector. Few
/// items are selected, so the items are kept in a sparse map. Keys are
/// generational, so items deleted since the last build are never returned
/// for the items reusing their slots.
#[derive(Debug, Default, Clone)]
pub struct SelectionRects {
    rects: Vec<Rect>,
    items: SparseSecondaryMap<LayoutInputKey, PaintSelectionItem>,
}

impl SelectionRects {
    pub fn new() -> Self {
        Self::default()
    }

    /// The selection of a text item. `None` if none of its text is
    /// selected.
    pub fn get(&self, key: LayoutInputKey) -> Option<&PaintSelectionItem> {
        self.items.get(key)
    }

    /// The rectangles behind the selected text of a text item, in the
    /// coordinates of the inline layout holding it. Empty if none of its
    /// text is selected.
    pub fn rects(&self, key: LayoutInputKey) -> &[Rect] {
        self.get(key)
            .and_then(|item| self.rects.get(item.rects.clone()))
            .unwrap_or_default()
    }

    /// Every rectangle, item after item.
    pub fn all(&self) -> &[Rect] {
        &self.rects
    }

    /// The selected text items with their selection, in no particular order.
    pub fn items(&self) -> impl Iterator<Item = (LayoutInputKey, &PaintSelectionItem)> {
        self.items.iter()
    }

    /// Whether no text is selected.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.rects.clear();
        self.items.clear();
    }

    /// Replace the rectangles with those of `selection` under `root`, from
    /// scratch, keeping the buffers. Each
    /// selected text item that can be
    /// [selected](crate::prepaint::LayoutInputData::selectable) in
    /// `prepaint` gets the rectangles parley highlights for its selected
    /// text. Without a selection, or with a collapsed one, no item has any.
    ///
    /// Rectangles follow the inline layouts, so build them after every
    /// layout while text is selected, and once more when the selection is
    /// cleared. `inputs` is the input tree `fragments` were built from, from
    /// the UI of `ui`, and `prepaint` is resolved for them.
    pub fn build(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        fragments: &FragmentTree,
        prepaint: &PrepaintTree,
        root: BoxFragmentKey,
        selection: Option<Selection>,
    ) {
        self.clear();
        let Some(ranges) = selection
            .and_then(|selection| SelectedRanges::new(ui, inputs, fragments, root, selection))
        else {
            return;
        };
        let Self { rects, items } = self;

        for range in ranges {
            let Some(inline) = fragments.inline_layout(range.owner) else {
                continue;
            };
            let layout = &inline.layout;
            let mut from = range.start.0;

            while let Some((item, text)) = next_text_item(inline, from, range.end.0) {
                let start = if text.start <= range.start.0 {
                    range.start
                } else {
                    (text.start, parley::Affinity::Downstream)
                };
                let end = if text.end >= range.end.0 {
                    range.end
                } else {
                    (text.end, parley::Affinity::Upstream)
                };
                from = end.0;

                if !prepaint.is_selectable(item) {
                    continue;
                }

                // A text item is in one inline layout, and in one of its
                // selected ranges, so its rectangles follow each other.
                let first = rects.len();
                // Indices past the text, such as of text edited since, end
                // it.
                let cursor =
                    |(index, affinity)| parley::Cursor::from_byte_index(layout, index, affinity);
                parley::Selection::new(cursor(start), cursor(end))
                    .geometry_with(layout, |rect, _| {
                        rects.push(Rect::new(rect.x0, rect.y0, rect.x1, rect.y1))
                    });

                if first < rects.len() {
                    items.insert(
                        item,
                        PaintSelectionItem {
                            rects: first..rects.len(),
                        },
                    );
                }
            }
        }
    }
}
