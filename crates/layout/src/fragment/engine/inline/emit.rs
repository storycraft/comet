//! Converting placed lines and blocks into fragments.

use rustc_hash::FxHashMap;
use slotmap::{SecondaryMap, SparseSecondaryMap};

use super::{super::Engine, Scratch, from_box_id, lines::Placed};
use crate::{
    fragment::{
        Fragment, FragmentBox, FragmentItem, FragmentKey, state::NodeState, tree::FragmentTree,
    },
    input::{LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
    slot_tree::SlotTree,
    style::geometry::{Point, Size},
};

type Layout = parley::Layout<Option<comet_tree::node::NodeKey>>;

impl Engine<'_> {
    /// Replace the fragments of an inline formatting context with the content
    /// placed by the last [`Self::break_inline`] call. `origin` is the content
    /// box offset from the border box.
    pub(super) fn emit_inline(&mut self, key: LayoutInputKey, origin: taffy::Point<f32>) {
        let fragment = self.fragments.states[key].fragment;
        self.release_children(fragment);

        let Scratch {
            events,
            spans,
            span_parents,
            links,
        } = &mut self.scratch.inline;
        let FragmentTree {
            items,
            inline_layouts,
            states,
            first,
            ..
        } = &mut *self.fragments;
        let layout = &inline_layouts[key].layout;

        let mut emitter = Emitter {
            tree: self.tree,
            root: key,
            fragment,
            origin,
            items,
            states,
            span_parents,
            spans,
            links,
            first,
            line: 0,
            line_key: fragment,
        };

        emitter.links.clear();

        for event in events.drain(..) {
            match event {
                Placed::Block { key, top } => emitter.block(key, top),
                Placed::Line(index) => emitter.line(layout, index),
            }
        }

        emitter.fit_spans();
        emitter.links.clear();
    }
}

/// Emits the fragments of one inline formatting context.
struct Emitter<'a> {
    tree: &'a LayoutInputTree,
    /// Item establishing the inline formatting context.
    root: LayoutInputKey,
    /// Box fragment of `root`.
    fragment: FragmentKey,
    /// Content box offset from the border box.
    origin: taffy::Point<f32>,
    items: &'a mut SlotTree<FragmentKey, Fragment>,
    states: &'a SecondaryMap<LayoutInputKey, NodeState>,
    span_parents: &'a mut FxHashMap<LayoutInputKey, FragmentKey>,
    spans: &'a mut Vec<FragmentKey>,
    links: &'a mut FxHashMap<LayoutInputKey, FragmentKey>,
    first: &'a mut SparseSecondaryMap<LayoutInputKey, FragmentKey>,
    /// Index of the current line in the Parley layout.
    line: usize,
    /// Fragment of the current line.
    line_key: FragmentKey,
}

impl Emitter<'_> {
    /// Attach a block box between lines, `top` being its margin box top.
    fn block(&mut self, key: LayoutInputKey, top: f32) {
        let state = &self.states[key];

        self.items[state.fragment].layout.location = Point::new(
            (self.origin.x + state.layout.location.x) as f64,
            (self.origin.y + top + state.layout.margin.top) as f64,
        );
        self.items.append(self.fragment, state.fragment);
    }

    /// Emit a line box and its content.
    fn line(&mut self, layout: &Layout, index: usize) {
        let Some(line) = layout.get(index) else {
            return;
        };
        let metrics = line.metrics();
        let left = metrics.inline_min_coord;
        let top = metrics.block_min_coord;

        self.line = index;
        self.line_key = self.items.insert(Fragment::new(
            self.root,
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
        run: &parley::GlyphRun<'_, Option<comet_tree::node::NodeKey>>,
        item: usize,
        (left, top, line_height): (f32, f32, f32),
    ) {
        // Line breaks around blocks belong to non-text spans.
        let Some(source) = run
            .style()
            .brush
            .and_then(|node| self.tree.item_for_node(node))
            .filter(|&source| {
                self.tree
                    .items
                    .get(source)
                    .is_some_and(|item| item.kind == LayoutInputKind::Text)
            })
        else {
            return;
        };

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
        // Blocks are placed between lines.
        if bx.kind == parley::InlineBoxKind::CustomOutOfFlow {
            return;
        }

        let child = from_box_id(bx.id);

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
        let state = &self.states[child];
        let margin = state.layout.margin;

        self.items[state.fragment].layout.location = Point::new(
            (bx.x + margin.left - left) as f64,
            (bx.y + margin.top - top) as f64,
        );
        self.items.append(parent, state.fragment);
    }

    /// Insert a linked fragment under the span containing `source`.
    fn push(&mut self, source: LayoutInputKey, item: FragmentItem, layout: FragmentBox) {
        let parent = self.parent(source);
        let key = self.items.insert(Fragment::new(source, item, layout));

        self.items.append(parent, key);
        self.link(source, key);
    }

    /// The fragment containing `source` on this line, creating span fragments
    /// for its inline ancestors.
    fn parent(&mut self, source: LayoutInputKey) -> FragmentKey {
        // A text root's own runs belong to its line, even if it has a parent.
        if source == self.root {
            return self.line_key;
        }

        let Some(key) = self
            .tree
            .items
            .parent(source)
            .filter(|&key| key != self.root)
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
        let fragment = self.items.insert(Fragment::new(
            key,
            FragmentItem::Inline { line: self.line },
            FragmentBox::default(),
        ));
        self.items.append(parent, fragment);
        self.link(key, fragment);
        self.spans.push(fragment);
        self.span_parents.insert(key, fragment);

        fragment
    }

    /// Chain fragments generated by the same source across lines.
    fn link(&mut self, source: LayoutInputKey, key: FragmentKey) {
        if let Some(prev) = self.links.insert(source, key) {
            self.items[prev].link.next = Some(key);
            self.items[key].link.prev = Some(prev);
        } else {
            self.first.insert(source, key);
        }
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
