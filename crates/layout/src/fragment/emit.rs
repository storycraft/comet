use comet_tree::node::NodeKey;
use rustc_hash::FxHashMap;
use slotmap::KeyData;

use super::{Fragment, FragmentBox, FragmentItem, FragmentLink};
use crate::{
    style::geometry::{Point, Size},
    taffy::{TaffyLayout, conv},
    tree::{ChildLayoutMode, LayoutItemKey, LayoutTree},
};

/// Flat output tree with direct child links and build-wide lookup tables.
#[derive(Default)]
pub(super) struct Desired {
    items: Vec<(Option<usize>, Fragment)>,
    first_child: Vec<Option<usize>>,
    next_sibling: Vec<Option<usize>>,
    spans: Vec<usize>,
    indices: FxHashMap<(usize, LayoutItemKey), usize>,
    sources: FxHashMap<(LayoutItemKey, NodeKey), LayoutItemKey>,
}

impl Desired {
    pub fn drain(&mut self) -> std::vec::Drain<'_, (Option<usize>, Fragment)> {
        self.first_child.clear();
        self.next_sibling.clear();
        self.indices.clear();
        self.sources.clear();

        self.items.drain(..)
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn push(&mut self, (parent, fragment): (Option<usize>, Fragment)) {
        let index = self.items.len();
        let next = parent.and_then(|parent| self.first_child[parent]);
        self.items.push((parent, fragment));
        self.first_child.push(None);
        self.next_sibling.push(next);

        if let Some(parent) = parent {
            self.first_child[parent] = Some(index);
        }
    }

    fn index_sources(&mut self, tree: &LayoutTree, root: LayoutItemKey, key: LayoutItemKey) {
        if let Some(node) = tree.items[key].ty.node() {
            self.sources.entry((root, node)).or_insert(key);
        }

        if key != root
            && matches!(
                tree.items[key].ty,
                crate::tree::LayoutItemTy::AtomicInline(_)
            )
        {
            return;
        }

        for child in tree.items.children(key) {
            self.index_sources(tree, root, child);
        }
    }

    /// Each edge is visited twice. Nested spans finish before their parents.
    pub fn finish(&mut self) {
        while let Some(span) = self.spans.pop() {
            let Some(first) = self.first_child[span] else {
                continue;
            };
            let mut child = Some(first);
            let mut left = f64::INFINITY;
            let mut top = f64::INFINITY;
            let mut right = f64::NEG_INFINITY;
            let mut bottom = f64::NEG_INFINITY;

            while let Some(index) = child {
                let f = &self.items[index].1.layout;
                left = left.min(f.location.x);
                top = top.min(f.location.y);
                right = right.max(f.location.x + f.size.width);
                bottom = bottom.max(f.location.y + f.size.height);
                child = self.next_sibling[index];
            }

            self.items[span].1.layout.location = Point::new(left, top);
            self.items[span].1.layout.size = Size::new(right - left, bottom - top);
            child = Some(first);

            while let Some(index) = child {
                self.items[index].1.layout.location.x -= left;
                self.items[index].1.layout.location.y -= top;
                child = self.next_sibling[index];
            }
        }
    }
}

fn fragment(
    source: LayoutItemKey,
    node: Option<NodeKey>,
    item: FragmentItem,
    location: Point<f64>,
    size: Size<f64>,
) -> Fragment {
    Fragment {
        source,
        node,
        item,
        layout: FragmentBox {
            location,
            size,
            ..Default::default()
        },
        link: FragmentLink::default(),
    }
}

pub(super) fn emit_box(
    engine: &TaffyLayout<'_>,
    key: LayoutItemKey,
    parent: Option<usize>,
    position: Option<Point<f64>>,
    out: &mut Desired,
) {
    let bx = engine.nodes[&key].layout;
    let index = out.len();
    let mut f = fragment(
        key,
        engine.tree.items[key].ty.node(),
        FragmentItem::Box,
        position.unwrap_or_else(|| conv::from_point(bx.location, f64::from)),
        conv::from_size(bx.size, f64::from),
    );
    f.layout.padding = conv::from_rect(bx.padding, f64::from);
    f.layout.border = conv::from_rect(bx.border, f64::from);

    if matches!(
        engine.tree.items[key].ty,
        crate::tree::LayoutItemTy::AtomicInline(_)
    ) {
        f.layout.baseline = Some(engine.inline_baseline(key) as f64);
    }

    out.push((parent, f));

    if matches!(
        engine.tree.items[key].child_layout,
        ChildLayoutMode::Block | ChildLayoutMode::Container
    ) {
        for child in engine.tree.items.children(key) {
            emit_box(engine, child, Some(index), None, out);
        }
    }

    let Some(inline) = engine.fragments.inline_layouts.get(&key) else {
        return;
    };
    out.index_sources(engine.tree, key, key);

    for (line_index, line) in inline.layout.lines().enumerate() {
        let metrics = line.metrics();
        let line_parent = out.len();
        let mut line_fragment = fragment(
            key,
            None,
            FragmentItem::Line { line: line_index },
            Point::new(
                (bx.border.left + bx.padding.left + metrics.inline_min_coord) as f64,
                (bx.border.top + bx.padding.top + metrics.block_min_coord) as f64,
            ),
            Size::new(
                (metrics.inline_max_coord - metrics.inline_min_coord) as f64,
                metrics.line_height as f64,
            ),
        );
        line_fragment.layout.baseline = Some((metrics.baseline - metrics.block_min_coord) as f64);
        out.push((Some(index), line_fragment));

        for (item_index, item) in line.items().enumerate() {
            match item {
                parley::PositionedLayoutItem::GlyphRun(run) => {
                    let node = run.style().brush;
                    let source = node
                        .and_then(|node| out.sources.get(&(key, node)).copied())
                        .unwrap_or(key);
                    let parent =
                        inline_parent(engine.tree, key, source, line_index, line_parent, out);
                    let mut f = fragment(
                        source,
                        node,
                        FragmentItem::GlyphRun {
                            line: line_index,
                            item: item_index,
                        },
                        Point::new((run.offset() - metrics.inline_min_coord) as f64, 0.0),
                        Size::new(run.advance() as f64, metrics.line_height as f64),
                    );
                    f.layout.baseline = Some((run.baseline() - metrics.block_min_coord) as f64);
                    out.push((Some(parent), f));
                }
                parley::PositionedLayoutItem::InlineBox(bx) => {
                    let child = LayoutItemKey::from(KeyData::from_ffi(bx.id));
                    let parent =
                        inline_parent(engine.tree, key, child, line_index, line_parent, out);
                    if matches!(
                        engine.tree.items[child].ty,
                        crate::tree::LayoutItemTy::Inline(_)
                    ) {
                        out.push((
                            Some(parent),
                            fragment(
                                child,
                                engine.tree.items[child].ty.node(),
                                FragmentItem::Inline { line: line_index },
                                Point::new((bx.x - metrics.inline_min_coord) as f64, 0.0),
                                Size::new(0.0, metrics.line_height as f64),
                            ),
                        ));

                        continue;
                    }

                    let margin = engine.nodes[&child].layout.margin;

                    emit_box(
                        engine,
                        child,
                        Some(parent),
                        Some(Point::new(
                            (bx.x + margin.left - metrics.inline_min_coord) as f64,
                            (bx.y + margin.top - metrics.block_min_coord) as f64,
                        )),
                        out,
                    );
                }
            }
        }
    }
}

fn inline_parent(
    tree: &LayoutTree,
    root: LayoutItemKey,
    source: LayoutItemKey,
    line: usize,
    line_parent: usize,
    out: &mut Desired,
) -> usize {
    let Some(key) = tree.items.parent(source).filter(|&key| key != root) else {
        return line_parent;
    };

    if let Some(&index) = out.indices.get(&(line_parent, key)) {
        return index;
    }

    let parent = inline_parent(tree, root, key, line, line_parent, out);

    if !matches!(tree.items[key].ty, crate::tree::LayoutItemTy::Inline(_)) {
        return parent;
    }

    let index = out.len();
    out.push((
        Some(parent),
        fragment(
            key,
            tree.items[key].ty.node(),
            FragmentItem::Inline { line },
            Point::default(),
            Size::default(),
        ),
    ));
    out.spans.push(index);
    out.indices.insert((line_parent, key), index);

    index
}
