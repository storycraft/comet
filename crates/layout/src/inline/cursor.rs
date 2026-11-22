use core::ops::Range;

use comet_slotmap_tree::SlotTree;
use kurbo::Point;

use super::InlinePosition;
use crate::{
    fragment::{FragmentItem, FragmentKey, InlineLayout, RunEntry, tree::FragmentTree},
    input::{LayoutInputItem, LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
};

/// A position in an inline layout, with the run containing it.
///
/// A run is a leaf fragment of a line: a glyph run, an atomic inline box, or
/// an empty inline element. Each run covers a range of the layout's text; an
/// index belongs to the run whose range contains it, so the index at a soft
/// line break belongs to the run starting the next line. The end of the
/// text belongs to the last run.
///
/// The cursor is owned and holds no borrow. Its methods take the
/// [`FragmentTree`] it was created from; passing another tree is a logic
/// error, which may return wrong results or panic.
///
/// The run is cached with the cursor. Building the fragment tree again
/// replaces the line fragments, which makes the cursor stale. Keeping it
/// current is up to the caller, for example by creating it again from its
/// [position](Self::position) with [`new`](Self::new) after a build. Using a
/// stale cursor is a logic error, like passing another tree.
///
/// Runs are ordered logically, by their text ranges. Moving to another run
/// places the cursor at the start of that run's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineCursor {
    position: InlinePosition,
    /// The current run.
    entry: RunEntry,
    /// Index of the current run in the owner's run index.
    run: usize,
    /// Line fragment containing the current run.
    line_fragment: FragmentKey,
}

impl InlineCursor {
    /// A cursor at `position`. `None` if its owner has no emitted inline
    /// layout, or the index is out of range or not on a char boundary.
    pub fn new(tree: &FragmentTree, position: InlinePosition) -> Option<Self> {
        let inline = tree.get_inline(position.owner)?;
        if !inline.text.is_char_boundary(position.index) {
            return None;
        }

        let run = find_run(&inline.runs, position.index)?;
        Self::at_run(position.owner, inline, run, position.index)
    }

    /// A cursor at the start of the first run of an inline layout.
    pub fn start(tree: &FragmentTree, owner: FragmentKey) -> Option<Self> {
        let inline = tree.get_inline(owner)?;
        let entry = inline.runs.first()?;

        Self::at_run(owner, inline, 0, entry.start)
    }

    /// A cursor at the end of the text of an inline layout, in its last run.
    pub fn end(tree: &FragmentTree, owner: FragmentKey) -> Option<Self> {
        let inline = tree.get_inline(owner)?;
        let run = inline.runs.len().checked_sub(1)?;

        Self::at_run(owner, inline, run, inline.text.len())
    }

    /// A cursor at the start of a run, given its leaf fragment: a glyph run,
    /// an atomic inline box placed on a line, or an empty inline element.
    pub fn from_fragment(tree: &FragmentTree, fragment: FragmentKey) -> Option<Self> {
        let items = &tree.items;
        let mut line = items.parent(fragment)?;
        while matches!(items.get(line)?.item, FragmentItem::Inline { .. }) {
            line = items.parent(line)?;
        }

        let FragmentItem::Line { line: index } = items[line].item else {
            return None;
        };
        let owner = items.parent(line)?;
        let inline = tree.get_inline(owner)?;
        if inline.lines.get(index) != Some(&line) {
            return None;
        }

        let run =
            line_runs(&inline.runs, index).find(|&run| inline.runs[run].fragment == fragment)?;
        Self::at_run(owner, inline, run, inline.runs[run].start)
    }

    /// A cursor at the caret position closest to `point` in a glyph run
    /// fragment, in the run's local coordinates.
    ///
    /// The caret is at the edge of the cluster under `point.x` closest to
    /// it, which may be the end of the run; `point.y` is ignored. `None` if
    /// `fragment` is not an emitted glyph run.
    pub fn from_point(tree: &FragmentTree, fragment: FragmentKey, point: Point) -> Option<Self> {
        let FragmentItem::GlyphRun { line, item } = tree.items.get(fragment)?.item else {
            return None;
        };
        let cursor = Self::from_fragment(tree, fragment)?;
        let inline = cursor.inline(tree)?;
        let parley::PositionedLayoutItem::GlyphRun(run) =
            inline.layout.get(line)?.items().nth(item)?
        else {
            return None;
        };

        // Clusters of the glyph run, from its left edge.
        let style = run.style_index();
        let x = point.x as f32;
        let mut left = 0.0;
        let mut index = cursor.entry.start;
        for cluster in run
            .run()
            .visual_clusters()
            .filter(|cluster| cluster.style_index() == style)
        {
            let range = cluster.text_range();
            let (leading, trailing) = if cluster.is_hard_line_break() {
                // A caret after a hard break would be on the next line.
                (range.start, range.start)
            } else if cluster.is_rtl() {
                (range.end, range.start)
            } else {
                (range.start, range.end)
            };

            if x < left + cluster.advance() / 2.0 {
                index = leading;
                break;
            }

            index = trailing;
            left += cluster.advance();
        }

        Some(Self {
            position: InlinePosition::new(cursor.position.owner, index),
            ..cursor
        })
    }

    /// A cursor at the start of the first run of an item, in logical order.
    ///
    /// Text items start at their first glyph run, and inline elements at the
    /// first run inside them, or their own run when empty. An atomic inline
    /// is the run of its box in the line containing it. Other boxes start at
    /// the first run of the inline layout they own, or of their first
    /// anonymous block.
    pub fn at_item(tree: &FragmentTree, key: LayoutInputKey) -> Option<Self> {
        let items = &tree.items;
        let first = *tree.first.get(key)?;
        let fragment = items.get(first).filter(|fragment| fragment.source == key)?;

        match fragment.item {
            FragmentItem::GlyphRun { .. } => Self::from_fragment(tree, first),
            FragmentItem::Inline { .. } if items.first_child(first).is_none() => {
                Self::from_fragment(tree, first)
            }
            FragmentItem::Inline { .. } => Self::first_in_span(tree, first),
            FragmentItem::Box { .. } => {
                if items.parent(first).is_some_and(|parent| {
                    matches!(
                        items[parent].item,
                        FragmentItem::Line { .. } | FragmentItem::Inline { .. }
                    )
                }) {
                    Self::from_fragment(tree, first)
                } else if tree.get_inline(first).is_some() {
                    Self::start(tree, first)
                } else {
                    items
                        .children(first)
                        .filter(|&child| items[child].item.is_anonymous_block())
                        .find_map(|child| Self::start(tree, child))
                }
            }
            _ => None,
        }
    }

    /// A cursor at another index of the same inline layout.
    pub fn with_index(&self, tree: &FragmentTree, index: usize) -> Option<Self> {
        Self::new(tree, InlinePosition::new(self.position.owner, index))
    }

    pub const fn position(&self) -> InlinePosition {
        self.position
    }

    /// Fragment owning the inline layout.
    pub const fn owner(&self) -> FragmentKey {
        self.position.owner
    }

    /// Byte offset into the inline layout's text.
    pub const fn index(&self) -> usize {
        self.position.index
    }

    /// Leaf fragment of the current run.
    pub const fn fragment(&self) -> FragmentKey {
        self.entry.fragment
    }

    /// Line fragment containing the current run.
    pub const fn line_fragment(&self) -> FragmentKey {
        self.line_fragment
    }

    /// Byte range of the current run in the inline layout's text.
    pub const fn text_range(&self) -> Range<usize> {
        self.entry.start..self.entry.end
    }

    /// The inline layout the cursor is in.
    pub fn inline<'a>(&self, tree: &'a FragmentTree) -> Option<&'a InlineLayout> {
        tree.get_inline(self.position.owner)
    }

    /// Item generating the current run.
    pub fn source(&self, tree: &FragmentTree) -> Option<LayoutInputKey> {
        Some(tree.items.get(self.entry.fragment)?.source)
    }

    /// The start of the next run, across lines.
    #[must_use]
    pub fn next_run(&self, tree: &FragmentTree) -> Option<Self> {
        self.with_run(tree, self.run + 1)
    }

    /// The start of the previous run, across lines.
    #[must_use]
    pub fn previous_run(&self, tree: &FragmentTree) -> Option<Self> {
        self.with_run(tree, self.run.checked_sub(1)?)
    }

    /// The first run of the next item among the siblings of the current
    /// run's item, skipping siblings without runs in this inline layout.
    /// `None` when the current run belongs to the owner's own item.
    #[must_use]
    pub fn next_sibling_item(&self, tree: &FragmentTree, inputs: &LayoutInputTree) -> Option<Self> {
        self.sibling_item(tree, inputs, SlotTree::next_sibling)
    }

    /// The first run of the previous item among the siblings of the current
    /// run's item, skipping siblings without runs in this inline layout.
    /// `None` when the current run belongs to the owner's own item.
    #[must_use]
    pub fn previous_sibling_item(
        &self,
        tree: &FragmentTree,
        inputs: &LayoutInputTree,
    ) -> Option<Self> {
        self.sibling_item(tree, inputs, SlotTree::prev_sibling)
    }

    /// The run of the next line with runs whose horizontal extent is
    /// closest to the center of the current run.
    #[must_use]
    pub fn next_line(&self, tree: &FragmentTree) -> Option<Self> {
        self.next_line_at(tree, self.center(tree)?)
    }

    /// The run of the previous line with runs whose horizontal extent is
    /// closest to the center of the current run.
    #[must_use]
    pub fn previous_line(&self, tree: &FragmentTree) -> Option<Self> {
        self.previous_line_at(tree, self.center(tree)?)
    }

    /// The run of the next line with runs whose horizontal extent is
    /// closest to `x`, in the coordinates of the inline layout.
    #[must_use]
    pub fn next_line_at(&self, tree: &FragmentTree, x: f32) -> Option<Self> {
        let inline = self.inline(tree)?;
        let next = inline
            .runs
            .partition_point(|entry| entry.line <= self.entry.line);
        let line = inline.runs.get(next)?.line;

        self.closest_on_line(inline, line, x)
    }

    /// The run of the previous line with runs whose horizontal extent is
    /// closest to `x`, in the coordinates of the inline layout.
    #[must_use]
    pub fn previous_line_at(&self, tree: &FragmentTree, x: f32) -> Option<Self> {
        let inline = self.inline(tree)?;
        let start = inline
            .runs
            .partition_point(|entry| entry.line < self.entry.line);
        let line = inline.runs[start.checked_sub(1)?].line;

        self.closest_on_line(inline, line, x)
    }

    /// The start of the logically first run of the current line.
    #[must_use]
    pub fn line_start(&self, tree: &FragmentTree) -> Option<Self> {
        let inline = self.inline(tree)?;
        let run = line_runs(&inline.runs, self.entry.line).next()?;

        self.with_run(tree, run)
    }

    /// The start of the logically last run of the current line.
    #[must_use]
    pub fn line_end(&self, tree: &FragmentTree) -> Option<Self> {
        let inline = self.inline(tree)?;
        let run = line_runs(&inline.runs, self.entry.line).next_back()?;

        self.with_run(tree, run)
    }

    fn at_run(owner: FragmentKey, inline: &InlineLayout, run: usize, index: usize) -> Option<Self> {
        let entry = *inline.runs.get(run)?;
        let line_fragment = *inline.lines.get(entry.line)?;

        Some(Self {
            position: InlinePosition::new(owner, index),
            entry,
            run,
            line_fragment,
        })
    }

    /// The start of another run of the same inline layout.
    fn with_run(&self, tree: &FragmentTree, run: usize) -> Option<Self> {
        let inline = self.inline(tree)?;
        Self::at_run(
            self.position.owner,
            inline,
            run,
            inline.runs.get(run)?.start,
        )
    }

    /// The logically first run inside an inline span fragment.
    fn first_in_span(tree: &FragmentTree, span: FragmentKey) -> Option<Self> {
        let items = &tree.items;
        let mut leaves = Vec::new();
        let mut pending = vec![span];

        while let Some(key) = pending.pop() {
            for child in items.children(key) {
                match items[child].item {
                    FragmentItem::Inline { .. } if items.first_child(child).is_some() => {
                        pending.push(child)
                    }
                    _ => leaves.push(child),
                }
            }
        }

        // Spans are split per line, so every leaf is on the same line.
        let any = Self::from_fragment(tree, *leaves.first()?)?;
        let inline = any.inline(tree)?;
        let run = line_runs(&inline.runs, any.entry.line)
            .find(|&run| leaves.contains(&inline.runs[run].fragment))?;

        any.with_run(tree, run)
    }

    fn sibling_item(
        &self,
        tree: &FragmentTree,
        inputs: &LayoutInputTree,
        step: fn(
            &SlotTree<LayoutInputKey, LayoutInputItem>,
            LayoutInputKey,
        ) -> Option<LayoutInputKey>,
    ) -> Option<Self> {
        let owner = tree.items.get(self.position.owner)?.source;
        let mut key = tree.items.get(self.entry.fragment)?.source;
        if key == owner {
            return None;
        }

        loop {
            key = step(&inputs.items, key)?;

            // Blocks interrupt the inline content, out of this layout.
            if inputs.items.get(key)?.kind == LayoutInputKind::Block {
                continue;
            }

            if let Some(cursor) = Self::at_item(tree, key)
                && cursor.position.owner == self.position.owner
            {
                return Some(cursor);
            }
        }
    }

    /// Horizontal center of the current run in the inline layout.
    fn center(&self, tree: &FragmentTree) -> Option<f32> {
        let inline = self.inline(tree)?;
        let (left, right) = item_extent(
            &inline
                .layout
                .get(self.entry.line)?
                .items()
                .nth(self.entry.item)?,
        );

        Some((left + right) / 2.0)
    }

    /// The start of the run of `line` whose horizontal extent is closest to
    /// `x`, preferring the logically first on ties.
    fn closest_on_line(&self, inline: &InlineLayout, line: usize, x: f32) -> Option<Self> {
        let extents: Vec<(f32, f32)> = inline
            .layout
            .get(line)?
            .items()
            .map(|item| item_extent(&item))
            .collect();
        let distance = |run: usize| {
            extents
                .get(inline.runs[run].item)
                .map_or(f32::INFINITY, |&(left, right)| {
                    if x < left {
                        left - x
                    } else if x > right {
                        x - right
                    } else {
                        0.0
                    }
                })
        };
        let run =
            line_runs(&inline.runs, line).min_by(|&a, &b| distance(a).total_cmp(&distance(b)))?;

        Self::at_run(self.position.owner, inline, run, inline.runs[run].start)
    }
}

/// The run containing `index`, or the last run starting before it, or the
/// first run.
fn find_run(runs: &[RunEntry], index: usize) -> Option<usize> {
    if runs.is_empty() {
        return None;
    }

    let after = runs.partition_point(|entry| entry.start <= index);
    if let Some(run) = after.checked_sub(1)
        && index < runs[run].end
    {
        return Some(run);
    }

    Some(after.saturating_sub(1))
}

/// Indices of the runs of `line` in a run index.
fn line_runs(runs: &[RunEntry], line: usize) -> Range<usize> {
    runs.partition_point(|entry| entry.line < line)
        ..runs.partition_point(|entry| entry.line <= line)
}

/// Horizontal extent of a positioned line item.
fn item_extent(item: &parley::PositionedLayoutItem<'_, Option<LayoutInputKey>>) -> (f32, f32) {
    match item {
        parley::PositionedLayoutItem::GlyphRun(run) => (run.offset(), run.offset() + run.advance()),
        parley::PositionedLayoutItem::InlineBox(bx) => (bx.x, bx.x + bx.width),
    }
}
