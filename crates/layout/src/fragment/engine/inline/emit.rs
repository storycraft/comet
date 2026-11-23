//! Converting lines into fragments.

use comet_slotmap_tree::SlotTree;
use rustc_hash::FxHashMap;
use slotmap::{SecondaryMap, SlotMap};

use super::{
    super::{Engine, PAINT_CHANGED},
    Scratch,
};
use crate::{
    dirty_map::DirtyMap,
    fragment::{
        Fragment, FragmentBox, FragmentItem, FragmentKey, InlineLayout, StateKey, state::NodeState,
        tree::FragmentTree,
    },
    input::{LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
    style::geometry::{Point, Size},
};

impl Engine<'_> {
    /// Replace the lines of a fragment owning them with its broken and aligned
    /// inline layout. `origin` is the content box offset from the border box.
    ///
    /// Returns the area covered by the emitted lines and their descendants,
    /// with the [overflow](FragmentBox::overflow) of each, in the fragment's
    /// local coordinates. `None` without lines.
    pub(super) fn emit(
        &mut self,
        fragment: FragmentKey,
        origin: taffy::Point<f32>,
    ) -> Option<kurbo::Rect> {
        self.release_children(fragment);

        let Scratch {
            spans,
            span_parents,
            lines,
            ..
        } = &mut self.scratch.inline;
        let dirty = &mut *self.dirty;
        let FragmentTree {
            items,
            states,
            first,
            ..
        } = &mut *self.fragments;
        let state = items[fragment].item.state()?;
        let InlineLayout { layout, boxes, .. } = states.get(state)?.inline.as_deref()?;

        lines.clear();

        let mut emitter = Emitter {
            tree: self.tree,
            owner: items[fragment].source,
            fragment,
            origin,
            items,
            states,
            first,
            boxes,
            span_parents,
            spans,
            dirty,
            lines,
            line: 0,
            line_key: fragment,
        };

        for (index, line) in layout.lines().enumerate() {
            emitter.line(index, &line);
        }

        emitter.fit_spans();
        let extent = emitter.fit_overflow();

        if let Some(inline) = states
            .get_mut(state)
            .and_then(|state| state.inline.as_deref_mut())
        {
            std::mem::swap(&mut inline.lines, lines);
        }

        extent
    }

    /// Chain the inline and glyph run fragments of each source across the
    /// lines of a fragment, including the anonymous blocks of mixed inline
    /// content. Nested boxes link their own lines.
    pub(in crate::fragment::engine) fn link_inline(&mut self, fragment: FragmentKey) {
        let FragmentTree {
            items,
            first,
            states,
            ..
        } = &mut *self.fragments;
        let links = &mut self.scratch.inline.links;
        let mut pending: Vec<FragmentKey> = items.children(fragment).rev().collect();

        links.clear();

        while let Some(key) = pending.pop() {
            match items[key].item {
                FragmentItem::Box { .. } => continue,
                FragmentItem::AnonymousBlock { .. } | FragmentItem::Line { .. } => {}
                FragmentItem::Inline { .. } | FragmentItem::GlyphRun { .. } => {
                    let source = items[key].source;
                    items[key].link = Default::default();

                    if let Some(prev) = links.insert(source, key) {
                        items[prev].link.next = Some(key);
                        items[key].link.prev = Some(prev);
                    } else if !first.get(source).is_some_and(|&first| {
                        // A text item generating a box keeps its box first.
                        items.get(first).is_some_and(|first| {
                            matches!(first.item, FragmentItem::Box { state } if states.contains_key(state))
                        })
                    }) {
                        first.insert(source, key);
                    }
                }
            }

            pending.extend(items.children(key).rev());
        }

        links.clear();
    }
}

/// Emits the lines of one fragment owning them.
struct Emitter<'a> {
    tree: &'a LayoutInputTree,
    /// Item establishing the inline formatting context.
    owner: LayoutInputKey,
    /// Fragment owning the lines.
    fragment: FragmentKey,
    /// Content box offset from the border box.
    origin: taffy::Point<f32>,
    items: &'a mut SlotTree<FragmentKey, Fragment>,
    states: &'a SlotMap<StateKey, NodeState>,
    first: &'a SecondaryMap<LayoutInputKey, FragmentKey>,
    /// Items of the inline boxes, by inline box id.
    boxes: &'a [LayoutInputKey],
    span_parents: &'a mut FxHashMap<LayoutInputKey, FragmentKey>,
    spans: &'a mut Vec<FragmentKey>,
    /// Items whose fragments perform layout are marked
    /// [`PAINT_CHANGED`] in it.
    dirty: &'a mut DirtyMap,
    /// Emitted line fragments, by line index.
    lines: &'a mut Vec<FragmentKey>,
    /// Index of the current line in the Parley layout.
    line: usize,
    /// Fragment of the current line.
    line_key: FragmentKey,
}

impl Emitter<'_> {
    /// Emit a line box and its content.
    fn line(&mut self, index: usize, line: &parley::Line<'_, Option<LayoutInputKey>>) {
        let metrics = line.metrics();
        let left = metrics.inline_min_coord;
        let top = metrics.block_min_coord;

        self.line = index;
        self.line_key = self.insert(Fragment::new(
            self.owner,
            FragmentItem::Line { line: index },
            FragmentBox {
                location: Point::new((self.origin.x + left) as f64, (self.origin.y + top) as f64),
                size: Size::new(
                    (metrics.inline_max_coord - left) as f64,
                    metrics.line_height as f64,
                ),
                baseline: Some((metrics.baseline - top) as f64),
                ..Default::default()
            },
        ));
        self.items.append(self.fragment, self.line_key);
        self.lines.push(self.line_key);
        self.span_parents.clear();

        // Line relative offsets of the line's content.
        let offset = (left, top, metrics.line_height);

        for (item_index, item) in line.items().enumerate() {
            match item {
                parley::PositionedLayoutItem::GlyphRun(run) => {
                    self.glyph_run(&run, item_index, offset)
                }
                parley::PositionedLayoutItem::InlineBox(bx) => self.inline_box(&bx, offset),
            }
        }
    }

    fn glyph_run(
        &mut self,
        run: &parley::GlyphRun<'_, Option<LayoutInputKey>>,
        item: usize,
        (left, top, line_height): (f32, f32, f32),
    ) {
        // Text is only pushed inside the span of its own item.
        let Some(source) = run.style().brush else {
            return;
        };
        debug_assert_eq!(self.tree.items[source].kind, LayoutInputKind::Text);

        let layout = FragmentBox {
            location: Point::new((run.offset() - left) as f64, 0.0),
            size: Size::new(run.advance() as f64, line_height as f64),
            baseline: Some((run.baseline() - top) as f64),
            ..Default::default()
        };

        self.push(
            source,
            FragmentItem::GlyphRun {
                line: self.line,
                item,
            },
            layout,
        );
    }

    fn inline_box(
        &mut self,
        bx: &parley::PositionedInlineBox,
        (left, top, line_height): (f32, f32, f32),
    ) {
        let child = self.boxes[bx.id as usize];

        // An empty inline element.
        if self.tree.items[child].kind == LayoutInputKind::Inline {
            let layout = FragmentBox {
                location: Point::new((bx.x - left) as f64, 0.0),
                size: Size::new(0.0, line_height as f64),
                ..Default::default()
            };

            self.push(child, FragmentItem::Inline { line: self.line }, layout);
            return;
        }

        let parent = self.parent(child);
        let atomic = self.first[child];
        let margin = self.items[atomic]
            .item
            .state()
            .and_then(|key| self.states.get(key))
            .map_or(taffy::Rect::ZERO, |state| state.layout.margin);

        let location = Point::new(
            (bx.x + margin.left - left) as f64,
            (bx.y + margin.top - top) as f64,
        );

        if self.items[atomic].layout.location != location {
            self.items[atomic].layout.location = location;
            self.dirty.mark(self.tree, child, PAINT_CHANGED);
        }
        self.items.append(parent, atomic);
    }

    /// Insert a new fragment, marking its item as laid out in this build.
    fn insert(&mut self, fragment: Fragment) -> FragmentKey {
        self.dirty.mark(self.tree, fragment.source, PAINT_CHANGED);
        self.items.insert(fragment)
    }

    /// Insert a fragment under the span containing `source`.
    fn push(
        &mut self,
        source: LayoutInputKey,
        item: FragmentItem,
        layout: FragmentBox,
    ) -> FragmentKey {
        let parent = self.parent(source);
        let key = self.insert(Fragment::new(source, item, layout));

        self.items.append(parent, key);
        key
    }

    /// The fragment containing `source` on this line, creating span fragments
    /// for its inline ancestors.
    fn parent(&mut self, source: LayoutInputKey) -> FragmentKey {
        // A text root's own runs belong to its line, even if it has a parent.
        if source == self.owner {
            return self.line_key;
        }

        let Some(key) = self
            .tree
            .items
            .parent(source)
            .filter(|&key| key != self.owner)
        else {
            return self.line_key;
        };

        if let Some(&fragment) = self.span_parents.get(&key) {
            return fragment;
        }

        let parent = self.parent(key);

        if self.tree.items[key].kind != LayoutInputKind::Inline {
            return parent;
        }

        // Spans are fitted to their contents after all lines are emitted.
        let fragment = self.insert(Fragment::new(
            key,
            FragmentItem::Inline { line: self.line },
            FragmentBox::default(),
        ));
        self.items.append(parent, fragment);
        self.spans.push(fragment);
        self.span_parents.insert(key, fragment);

        fragment
    }

    /// Set the [overflow](FragmentBox::overflow) of the emitted lines, spans
    /// and glyph runs to their extents, after the spans are fitted. Atomic
    /// inlines keep the overflow of their own layout. Returns the union of
    /// the lines' extents.
    fn fit_overflow(&mut self) -> Option<kurbo::Rect> {
        let mut extent: Option<kurbo::Rect> = None;
        let mut line = self.items.first_child(self.fragment);

        while let Some(key) = line {
            let line_extent = fit_overflow(self.items, key);
            extent = Some(extent.map_or(line_extent, |extent| extent.union(line_extent)));
            line = self.items.next_sibling(key);
        }

        extent
    }

    /// Fit inline spans to their contents. Nested spans finish before their parents.
    fn fit_spans(&mut self) {
        while let Some(span) = self.spans.pop() {
            let mut left = f64::INFINITY;
            let mut top = f64::INFINITY;
            let mut right = f64::NEG_INFINITY;
            let mut bottom = f64::NEG_INFINITY;

            for child in self.items.children(span) {
                let layout = &self.items[child].layout;
                left = left.min(layout.location.x);
                top = top.min(layout.location.y);
                right = right.max(layout.location.x + layout.size.width);
                bottom = bottom.max(layout.location.y + layout.size.height);
            }

            if left > right {
                continue;
            }

            let layout = &mut self.items[span].layout;
            layout.location = Point::new(left, top);
            layout.size = Size::new(right - left, bottom - top);

            let mut child = self.items.first_child(span);
            while let Some(key) = child {
                let layout = &mut self.items[key].layout;
                layout.location.x -= left;
                layout.location.y -= top;
                child = self.items.next_sibling(key);
            }
        }
    }
}

/// Set the [overflow](FragmentBox::overflow) of an emitted line, span or
/// glyph run, and of its descendants, to cover its border box and the
/// extents of its children. Boxes keep the overflow of their own layout.
/// Returns its extent in its parent's coordinates.
fn fit_overflow(items: &mut SlotTree<FragmentKey, Fragment>, key: FragmentKey) -> kurbo::Rect {
    if !items[key].item.is_box() {
        let mut overflow = items[key].layout.border_box();
        let mut child = items.first_child(key);

        while let Some(next) = child {
            overflow = overflow.union(fit_overflow(items, next));
            child = items.next_sibling(next);
        }

        items[key].layout.overflow = overflow;
    }

    items[key].layout.extent()
}
