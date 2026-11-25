//! Emitting the inline items of a broken and aligned inline layout.
//!
//! Items are rebuilt in place, in preorder: each line, then the pieces of
//! the inline elements open on it, glyph runs and boxes. Items of one source
//! are linked across lines, and the boxes inside are placed.

use comet_slotmap_tree::SlotTree;
use rustc_hash::FxHashMap;
use taffy::style::CoreStyle;

use super::{Engine, PAINT_CHANGED, Scratch, taffy::node_id};
use crate::{
    dirty_map::DirtyMap,
    fragment2::{
        BoxFragment, BoxFragmentKey, BoxFragmentType, ChildLayoutType, FragmentGeometry,
        InlineItem, InlineItemKind, InlineLink, inline_box_source,
    },
    input::{LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
    style::geometry::{Point, Size},
};

/// A piece of an inline element open on the current line.
#[derive(Debug, Clone, Copy)]
pub(super) struct Piece {
    source: LayoutInputKey,
    /// Index of its item.
    index: usize,
    /// Union of the border boxes of its children, in the owner's
    /// coordinates.
    rect: Option<kurbo::Rect>,
    /// Union of the extents of its children, in the owner's coordinates.
    extent: Option<kurbo::Rect>,
}

impl Engine<'_> {
    /// Replace the inline items of `key` with its broken and aligned lines,
    /// and place the boxes inside. `origin` is the content box offset from
    /// the border box.
    ///
    /// Returns the area covered by the lines and their content, in the local
    /// coordinates of the box. `None` without lines.
    pub(super) fn emit(&mut self, key: BoxFragmentKey, origin: kurbo::Vec2) -> Option<kurbo::Rect> {
        let fragment = self.fragments.get_mut(key)?;
        let owner = fragment.source;
        // Taken out while placing the boxes inside.
        let mut inline =
            match std::mem::replace(&mut fragment.child_layout, ChildLayoutType::Content) {
                ChildLayoutType::Inline(inline) => inline,
                other => {
                    fragment.child_layout = other;
                    return None;
                }
            };

        let tree = self.cx.tree;
        self.dirty.mark(tree, owner, PAINT_CHANGED);

        let mut items = std::mem::take(&mut inline.items);
        items.clear();
        let Scratch {
            links,
            open,
            chain,
            child_candidates,
            candidates,
            order,
            ..
        } = &mut *self.scratch;
        links.clear();
        *candidates = taffy::OofCandidates::new();
        order.clear();
        order.extend(
            self.fragments
                .boxes
                .children(key)
                .enumerate()
                .map(|(index, child)| (child, index as u32)),
        );

        let mut emitter = Emitter {
            tree,
            owner,
            items: &mut items,
            open,
            chain,
            links,
            boxes: &mut self.fragments.boxes,
            dirty: &mut *self.dirty,
            child_candidates,
            candidates,
            order,
            line_extent: kurbo::Rect::ZERO,
            line_baseline: 0.0,
        };
        let mut extent: Option<kurbo::Rect> = None;

        for (index, line) in inline.layout.lines().enumerate() {
            let line_number = index as u32;
            let metrics = line.metrics();
            let top = metrics.block_min_coord;
            let rect = kurbo::Rect::new(
                origin.x + f64::from(metrics.inline_min_coord),
                origin.y + f64::from(top),
                origin.x + f64::from(metrics.inline_max_coord),
                origin.y + f64::from(top + metrics.line_height),
            );
            let line_index = emitter.begin_line(line_number, rect, metrics.baseline - top);

            for (item_index, item) in line.items().enumerate() {
                match item {
                    parley::PositionedLayoutItem::GlyphRun(run) => {
                        // Runs without a brand belong to the owner.
                        let source = run.style().brush.unwrap_or(owner);
                        emitter.open_chain(source, false);

                        let x = origin.x + f64::from(run.offset());
                        let rect =
                            kurbo::Rect::new(x, rect.y0, x + f64::from(run.advance()), rect.y1);
                        emitter.push_leaf(
                            source,
                            InlineItemKind::Run {
                                line: line_number,
                                run: item_index as u32,
                            },
                            rect,
                            rect,
                            run.baseline() - top,
                        );
                    }
                    parley::PositionedLayoutItem::InlineBox(inline_box) => {
                        let source = inline_box_source(inline_box.id);
                        let position = kurbo::Point::new(
                            origin.x + f64::from(inline_box.x),
                            origin.y + f64::from(inline_box.y),
                        );

                        match inline.atomics.get(&source).copied() {
                            Some(child)
                                if emitter.boxes.get(child).is_some_and(|fragment| {
                                    fragment.box_type == BoxFragmentType::OutOfFlow
                                }) =>
                            {
                                // Out-of-flow boxes start at the top of the line.
                                emitter.out_of_flow(child, kurbo::Point::new(position.x, rect.y0));
                            }
                            Some(child) => emitter.place_box(source, child, position),
                            // An empty inline element.
                            None => emitter.empty_element(source, position, rect),
                        }
                    }
                }
            }

            let line_extent = emitter.end_line(line_index);
            extent = Some(extent.map_or(line_extent, |extent| extent.union(line_extent)));
        }

        inline.items = items;
        inline.origin = origin;
        self.fragments.boxes[key].child_layout = ChildLayoutType::Inline(inline);

        extent
    }
}

/// Emits the items of one inline layout.
struct Emitter<'a> {
    tree: &'a LayoutInputTree,
    /// Item establishing the inline formatting context.
    owner: LayoutInputKey,
    items: &'a mut Vec<InlineItem>,
    /// Pieces of the inline elements open on the current line, outermost
    /// first.
    open: &'a mut Vec<Piece>,
    /// Inline ancestors of the current item, outermost first.
    chain: &'a mut Vec<LayoutInputKey>,
    /// Index of the latest item of each source.
    links: &'a mut FxHashMap<LayoutInputKey, usize>,
    boxes: &'a mut SlotTree<BoxFragmentKey, BoxFragment>,
    dirty: &'a mut DirtyMap,
    /// Out-of-flow candidates left by each in-flow box inside, relative to
    /// its border box.
    child_candidates: &'a mut FxHashMap<BoxFragmentKey, taffy::OofCandidates>,
    /// Out-of-flow candidates of the inline content, relative to the
    /// owner's border box, in document order.
    candidates: &'a mut taffy::OofCandidates,
    /// Index of each child among the children of the owner.
    order: &'a FxHashMap<BoxFragmentKey, u32>,
    /// Union of the extents of the current line's content.
    line_extent: kurbo::Rect,
    /// Baseline of the current line, in the owner's coordinates.
    line_baseline: f64,
}

impl Emitter<'_> {
    /// Push a line item, returning its index.
    fn begin_line(&mut self, line: u32, rect: kurbo::Rect, baseline: f32) -> usize {
        let index = self.items.len();
        self.items.push(InlineItem {
            source: self.owner,
            kind: InlineItemKind::Line { line },
            descendants: 0,
            link: InlineLink::default(),
            geometry: geometry(rect, rect),
            baseline,
        });
        self.open.clear();
        self.line_extent = rect;
        self.line_baseline = rect.y0 + f64::from(baseline);

        index
    }

    /// Close the pieces open on the line, and finish its item. Returns its
    /// extent.
    fn end_line(&mut self, index: usize) -> kurbo::Rect {
        while !self.open.is_empty() {
            self.close_piece();
        }

        let extent = self.line_extent;
        let descendants = (self.items.len() - index - 1) as u32;
        let item = &mut self.items[index];
        item.descendants = descendants;
        item.geometry.overflow = local(extent, &item.geometry);

        extent
    }

    /// Make the pieces open on the line those of the inline ancestors of
    /// `source`, and of `source` itself with `include`, closing the others
    /// and opening the missing ones.
    fn open_chain(&mut self, source: LayoutInputKey, include: bool) {
        self.chain.clear();

        if include {
            self.chain.push(source);
        }

        // A text root's own runs belong to its lines.
        if source != self.owner {
            let mut ancestor = self.tree.items.parent(source);

            while let Some(key) = ancestor
                && key != self.owner
            {
                if self.tree.items[key].kind == LayoutInputKind::Inline {
                    self.chain.push(key);
                }

                ancestor = self.tree.items.parent(key);
            }
        }

        self.chain.reverse();

        let common = self
            .open
            .iter()
            .zip(self.chain.iter())
            .take_while(|(piece, source)| piece.source == **source)
            .count();

        while self.open.len() > common {
            self.close_piece();
        }

        for position in common..self.chain.len() {
            let source = self.chain[position];
            let index = self.push_item(InlineItem {
                source,
                kind: InlineItemKind::Box { child: None },
                descendants: 0,
                link: InlineLink::default(),
                geometry: FragmentGeometry::default(),
                baseline: 0.0,
            });
            self.open.push(Piece {
                source,
                index,
                rect: None,
                extent: None,
            });
        }
    }

    /// Finish the innermost open piece: fit it to its children.
    fn close_piece(&mut self) {
        let Some(piece) = self.open.pop() else {
            return;
        };

        let rect = piece.rect.unwrap_or_else(|| {
            let point = kurbo::Point::new(self.items[piece.index].geometry.location.x, 0.0);
            kurbo::Rect::from_points(point, point)
        });
        let extent = piece.extent.map_or(rect, |extent| extent.union(rect));
        let descendants = (self.items.len() - piece.index - 1) as u32;
        let item = &mut self.items[piece.index];

        item.descendants = descendants;
        item.geometry = geometry(rect, extent);
        item.baseline = (self.line_baseline - rect.y0) as f32;

        self.add_to_parent(rect, extent);
    }

    /// Add a finished child to the innermost open piece, or to the line.
    fn add_to_parent(&mut self, rect: kurbo::Rect, extent: kurbo::Rect) {
        match self.open.last_mut() {
            Some(piece) => {
                piece.rect = Some(piece.rect.map_or(rect, |old| old.union(rect)));
                piece.extent = Some(piece.extent.map_or(extent, |old| old.union(extent)));
            }
            None => self.line_extent = self.line_extent.union(extent),
        }
    }

    /// Push a glyph run or box item inside the open pieces.
    fn push_leaf(
        &mut self,
        source: LayoutInputKey,
        kind: InlineItemKind,
        rect: kurbo::Rect,
        extent: kurbo::Rect,
        baseline: f32,
    ) {
        self.push_item(InlineItem {
            source,
            kind,
            descendants: 0,
            link: InlineLink::default(),
            geometry: geometry(rect, extent),
            baseline,
        });
        self.add_to_parent(rect, extent);
    }

    /// Place the box `child` of `source` at the margin box position
    /// `position`, and push its
    /// item.
    fn place_box(&mut self, source: LayoutInputKey, child: BoxFragmentKey, position: kurbo::Point) {
        self.open_chain(source, false);

        let Some(fragment) = self.boxes.get_mut(child) else {
            return;
        };
        let margin = fragment.layout.unrounded.margin;
        let location = Point::new(
            position.x + f64::from(margin.left),
            position.y + f64::from(margin.top),
        );

        if fragment.layout.geometry.location != location {
            fragment.layout.geometry.location = location;
            self.dirty.mark(self.tree, source, PAINT_CHANGED);
        }

        if let Some(mut candidates) = self.child_candidates.remove(&child) {
            candidates.translate(taffy::Point {
                x: location.x as f32,
                y: location.y as f32,
            });
            self.candidates.append(&mut candidates);
        }

        let Some(fragment) = self.boxes.get(child) else {
            return;
        };
        let geometry = fragment.layout.geometry;
        let baseline = fragment
            .layout
            .baseline
            .unwrap_or(geometry.size.height as f32);
        let rect = kurbo::Rect::new(
            location.x,
            location.y,
            location.x + geometry.size.width,
            location.y + geometry.size.height,
        );

        self.push_item(InlineItem {
            source,
            kind: InlineItemKind::Box { child: Some(child) },
            descendants: 0,
            link: InlineLink::default(),
            geometry,
            baseline,
        });
        self.add_to_parent(rect, geometry.extent());
    }

    /// Record the out-of-flow box `child` as a Taffy candidate whose static
    /// position is `position`, for its containing block to lay out. It has
    /// no item: it is placed after the lines, and only found among the
    /// children of the owner.
    fn out_of_flow(&mut self, child: BoxFragmentKey, position: kurbo::Point) {
        let Some(fragment) = self.boxes.get(child) else {
            return;
        };

        self.candidates.push(taffy::OofCandidate {
            node: node_id(child),
            order: self.order.get(&child).copied().unwrap_or(0),
            position: fragment.style.position(),
            static_position: taffy::Point {
                x: taffy::AxisStaticPosition::from_edge(
                    position.x as f32,
                    taffy::AxisStaticEdge::Start,
                ),
                y: taffy::AxisStaticPosition::from_edge(
                    position.y as f32,
                    taffy::AxisStaticEdge::Start,
                ),
            },
        });
    }

    /// Open the piece of an empty inline element, as wide as nothing at
    /// `position` and as tall as the line `line`.
    fn empty_element(&mut self, source: LayoutInputKey, position: kurbo::Point, line: kurbo::Rect) {
        self.open_chain(source, true);

        if let Some(piece) = self.open.last_mut() {
            let rect = kurbo::Rect::new(position.x, line.y0, position.x, line.y1);
            piece.rect = Some(piece.rect.map_or(rect, |old| old.union(rect)));
        }
    }

    /// Push an item, linking it to the previous item of its source, and
    /// marking its source [`PAINT_CHANGED`]. Returns its index.
    fn push_item(&mut self, item: InlineItem) -> usize {
        let index = self.items.len();
        let source = item.source;
        self.items.push(item);
        self.dirty.mark(self.tree, source, PAINT_CHANGED);

        if let Some(prev) = self.links.insert(source, index) {
            let delta = (index - prev) as i32;
            self.items[prev].link.next = delta;
            self.items[index].link.prev = -delta;
        }

        index
    }
}

/// Geometry of an item covering `rect`, whose content reaches `extent`,
/// both in the owner's coordinates.
fn geometry(rect: kurbo::Rect, extent: kurbo::Rect) -> FragmentGeometry {
    let mut geometry = FragmentGeometry {
        location: Point::new(rect.x0, rect.y0),
        size: Size::new(rect.width(), rect.height()),
        ..Default::default()
    };
    geometry.overflow = local(extent.union(rect), &geometry);

    geometry
}

/// `extent`, in the owner's coordinates, in the local coordinates of an item
/// with `geometry`, covering its border box.
fn local(extent: kurbo::Rect, geometry: &FragmentGeometry) -> kurbo::Rect {
    let offset = kurbo::Vec2::new(geometry.location.x, geometry.location.y);

    (extent - offset).union(geometry.border_box())
}
