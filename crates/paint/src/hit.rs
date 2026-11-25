//! Hit testing of painted fragments, finding the fragment under a point in
//! the order fragments paint in.
//!
//! [`PaintHitTester::cursor`] walks the path to that fragment, from the owner
//! of the root layer down through its [paint parents](paint_parent), with the
//! point in the local coordinates of each fragment on it, such as for
//! dispatching an event through it. [`PaintHitTester::hit`] walks it to its
//! end.
//!
//! Boxes are hit by [`PrepaintTree::hit_local`], following their rounded
//! border boxes. Glyph runs are hit by
//! [`InlineLayout::hit_local`](comet_layout::fragment2::InlineLayout::hit_local),
//! and pieces of inline elements by the inline part boxes decorating them,
//! returned as the items of the pieces.

use comet_layout::fragment2::{BoxFragmentKey, FragmentRef, FragmentTree};
use kurbo::{Point, Vec2};

use crate::{
    layer::{
        PaintLayerKey,
        tree::{PaintLayerTree, layer_owner},
    },
    prepaint::{
        fragment::{
            BoxRole, for_each_paint_child_on_lines, paint_parent, part_of_piece, piece_of_part,
        },
        tree::PrepaintTree,
    },
};

/// The fragment found under a point by [`PaintHitTester::hit`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitTestResult {
    /// The fragment hit: a box, or an inline item, such as a glyph run or a
    /// piece of an inline element.
    pub fragment: FragmentRef,
    /// The point in the local coordinates of [`Self::fragment`], with its
    /// border box's top-left corner at the origin, after mapping it through
    /// the transforms of the boxes holding it.
    pub position: Point,
}

/// Finds the fragment painted on top under a point. Keep it to reuse its
/// buffers across hit tests.
#[derive(Default)]
pub struct PaintHitTester {
    /// Fragments to test in the layer being searched.
    pending: Vec<Visit>,
    /// Ancestors of a box, innermost first.
    chain: Vec<BoxFragmentKey>,
    /// Fragments left on the path walked by a [`PaintHitCursor`], innermost
    /// first.
    path: Vec<FragmentRef>,
}

/// A step of searching the fragments of a layer, in reverse paint order.
#[derive(Clone, Copy)]
struct Visit {
    fragment: FragmentRef,
    /// The point in the coordinates the fragment is placed in: the scrolled
    /// contents of its parent box, or of the box owning the inline layout of
    /// an inline item. Once the children of a box are queued, the point in
    /// its local coordinates.
    point: Point,
    /// Whether the fragment's children were queued, so it is tested next.
    expanded: bool,
}

/// Boxes of a layer whose contents hold the point at a known position, which
/// the fragments of the layer are found from: the owner of a box layer, or
/// the boxes holding the inline content of an inline element's layer.
#[derive(Clone, Copy)]
struct Bases {
    boxes: [(BoxFragmentKey, Point); 2],
    len: usize,
}

impl Bases {
    fn one(key: BoxFragmentKey, point: Point) -> Self {
        Self {
            boxes: [(key, point); 2],
            len: 1,
        }
    }

    fn get(&self, key: BoxFragmentKey) -> Option<Point> {
        self.boxes[..self.len]
            .iter()
            .find(|(base, _)| *base == key)
            .map(|&(_, point)| point)
    }
}

/// What hit testing reads from.
#[derive(Clone, Copy)]
struct Sources<'a> {
    layers: &'a PaintLayerTree,
    fragments: &'a FragmentTree,
    prepaint: &'a PrepaintTree,
}

impl PaintHitTester {
    pub fn new() -> Self {
        Self::default()
    }

    /// The topmost fragment under `point`, in scene coordinates, among the
    /// layers of `root` in `layers`, with `point` in its local coordinates,
    /// or `None` if no fragment is under it: the end of the path walked by
    /// [`cursor`](Self::cursor).
    ///
    /// Descendants of a scroll container are hit where they paint, moved
    /// against its scroll offset, as are those of the root's ancestors.
    ///
    /// `prepaint` must be resolved for `fragments`, and `layers` built for
    /// them.
    pub fn hit(
        &mut self,
        layers: &PaintLayerTree,
        fragments: &FragmentTree,
        prepaint: &PrepaintTree,
        root: PaintLayerKey,
        point: Point,
    ) -> Option<HitTestResult> {
        let mut cursor = self.cursor(root, point);
        let mut last = None;

        while let Some(hit) = cursor.hit_next(layers, fragments, prepaint) {
            last = Some(hit);
        }

        last
    }

    /// A cursor walking the path to the topmost fragment under `point`, in
    /// scene coordinates, among the layers of `root`: the owner of `root`,
    /// or the piece of its inline element holding the fragment, then each
    /// fragment down to the one hit, through its paint parents, whether or
    /// not they are under the point themselves. The trees are passed to
    /// [`PaintHitCursor::hit_next`].
    pub fn cursor(&mut self, root: PaintLayerKey, point: Point) -> PaintHitCursor<'_> {
        PaintHitCursor {
            tester: self,
            root,
            point,
            started: false,
        }
    }

    /// The topmost fragment under `point`, in scene coordinates, with
    /// `point` in its local coordinates.
    fn topmost(
        &mut self,
        sources: Sources<'_>,
        root: PaintLayerKey,
        point: Point,
    ) -> Option<HitTestResult> {
        let owner = sources.layers.layers.get(root)?.owner;
        let point = sources.placement(FragmentRef::Box(owner), point)?;

        self.pending.clear();
        self.hit_layer(sources, root, point)
    }

    /// The topmost fragment of a layer under `point`, in the coordinates its
    /// owner is placed in.
    fn hit_layer(
        &mut self,
        sources: Sources<'_>,
        key: PaintLayerKey,
        point: Point,
    ) -> Option<HitTestResult> {
        let owner = sources.layers.layers.get(key)?.owner;

        if BoxRole::of(sources.fragments.get(owner)?) == BoxRole::InlinePart {
            return self.hit_inline_layer(sources, key, owner, point);
        }

        let point = sources.local(owner, point)?;
        // The owner's clip bounds its descendants, child layers included.
        let inside = sources.inside_clip(owner, point);
        // Descendants, child layers included, scroll with the owner's
        // contents; its clip does not.
        let contents = point + sources.scroll(owner);
        let bases = Bases::one(owner, contents);

        if inside {
            // The last child layer with a negative z index, where the search
            // of negative layers resumes after the contents.
            let mut negative = None;

            for child in sources.layers.layers.children(key).rev() {
                if sources.layers.layers[child].z_index < 0 {
                    negative = Some(child);
                    break;
                }

                if let Some(hit) = self.hit_child(sources, child, &bases) {
                    return Some(hit);
                }
            }

            if sources.inside_overflow(owner, point)
                && let Some(hit) = self.hit_contents(sources, FragmentRef::Box(owner), contents)
            {
                return Some(hit);
            }

            while let Some(child) = negative {
                if let Some(hit) = self.hit_child(sources, child, &bases) {
                    return Some(hit);
                }

                negative = sources.layers.layers.prev_sibling(child);
            }
        }

        sources
            .prepaint
            .hit_local(sources.fragments, owner, point)
            .then_some(HitTestResult {
                fragment: FragmentRef::Box(owner),
                position: point,
            })
    }

    /// The topmost fragment of the layer of an inline element, owned by its
    /// first inline part box `owner`, under `point`, in the coordinates of
    /// the contents of the box holding `owner`. The layer covers every piece
    /// of the element, which do not clip.
    fn hit_inline_layer(
        &mut self,
        sources: Sources<'_>,
        key: PaintLayerKey,
        owner: BoxFragmentKey,
        point: Point,
    ) -> Option<HitTestResult> {
        let fragments = sources.fragments;
        let holder = fragments.boxes.parent(owner)?;
        let mut bases = Bases::one(holder, point);

        // Pieces in other runs are found from the box holding the runs. Run
        // boxes are only placed, so their local coordinates are those of
        // their contents.
        if fragments
            .get(holder)
            .is_some_and(|fragment| BoxRole::of(fragment) == BoxRole::Anonymous)
            && let Some(outer) = fragments.boxes.parent(holder)
            && let Some(data) = sources.prepaint.fragment(holder)
        {
            bases.boxes[1] = (outer, data.transform * point);
            bases.len = 2;
        }

        let last = fragments.split_boxes(owner).last();
        let mut negative = None;

        for child in sources.layers.layers.children(key).rev() {
            if sources.layers.layers[child].z_index < 0 {
                negative = Some(child);
                break;
            }

            if let Some(hit) = self.hit_child(sources, child, &bases) {
                return Some(hit);
            }
        }

        // The items of each piece, from the last.
        let mut part = last;
        while let Some(key) = part {
            part = fragments.get(key).and_then(|fragment| fragment.link.prev);

            if let Some((piece, parent, local)) = sources.piece_point(&bases, key)
                && sources.inside_overflow(key, local)
                && let Some(hit) = self.hit_contents(sources, piece, parent)
            {
                return Some(hit);
            }
        }

        while let Some(child) = negative {
            if let Some(hit) = self.hit_child(sources, child, &bases) {
                return Some(hit);
            }

            negative = sources.layers.layers.prev_sibling(child);
        }

        // Then the pieces themselves, by their inline part boxes.
        let mut part = last;
        while let Some(key) = part {
            part = fragments.get(key).and_then(|fragment| fragment.link.prev);

            if let Some((piece, _, local)) = sources.piece_point(&bases, key)
                && sources.prepaint.hit_local(fragments, key, local)
            {
                return Some(HitTestResult {
                    fragment: piece,
                    position: local,
                });
            }
        }

        None
    }

    /// The topmost fragment of a child layer of a layer whose boxes in
    /// `bases` hold the point at a known position, inside the clips of the
    /// boxes between its owner and a base and scrolled by them.
    fn hit_child(
        &mut self,
        sources: Sources<'_>,
        child: PaintLayerKey,
        bases: &Bases,
    ) -> Option<HitTestResult> {
        let child_owner = sources.layers.layers.get(child)?.owner;

        // Boxes between the owner and a base, innermost first.
        self.chain.clear();
        let mut next = sources.fragments.boxes.parent(child_owner);

        // An outdated layer not under a base is not hit.
        let mut point = loop {
            let key = next?;

            if let Some(point) = bases.get(key) {
                break point;
            }

            self.chain.push(key);
            next = sources.fragments.boxes.parent(key);
        };

        for &key in self.chain.iter().rev() {
            point = sources.local(key, point)?;

            if !sources.inside_clip(key, point) {
                return None;
            }

            point += sources.scroll(key);
        }

        self.hit_layer(sources, child, point)
    }

    /// The topmost descendant of `root` under `point`, in the coordinates its
    /// children are placed in, skipping the fragments starting child layers
    /// and their descendants.
    fn hit_contents(
        &mut self,
        sources: Sources<'_>,
        root: FragmentRef,
        point: Point,
    ) -> Option<HitTestResult> {
        let base = self.pending.len();
        self.push_children(sources, root, point);

        while self.pending.len() > base {
            let Visit {
                fragment,
                point,
                expanded,
            } = self.pending.pop()?;

            if expanded {
                if let Some(hit) = sources.hit_self(fragment, point) {
                    self.pending.truncate(base);
                    return Some(hit);
                }

                continue;
            }

            // Child layers are searched on their own, and the layer of an
            // inline element with every piece of it.
            if sources.starts_layer(fragment) {
                continue;
            }

            match fragment {
                FragmentRef::Box(key) => {
                    // Collapsed or unresolved boxes are not hit with their
                    // descendants.
                    let Some(local) = sources.local(key, point) else {
                        continue;
                    };

                    self.pending.push(Visit {
                        fragment,
                        point: local,
                        expanded: true,
                    });

                    if sources.inside_overflow(key, local) && sources.inside_clip(key, local) {
                        let contents = local + sources.scroll(key);
                        self.push_children(sources, fragment, contents);
                    }
                }
                FragmentRef::Inline { owner, index } => {
                    self.pending.push(Visit {
                        fragment,
                        point,
                        expanded: true,
                    });

                    // The items of a piece are placed in the same box.
                    let inside = sources
                        .fragments
                        .inline_layout(owner)
                        .and_then(|inline| inline.items.get(index as usize))
                        .is_some_and(|item| {
                            !item.is_run() && contains(item.geometry.extent(), point)
                        });

                    if inside {
                        self.push_children(sources, fragment, point);
                    }
                }
            }
        }

        None
    }

    /// Queue the children of `fragment` in paint order, placed at `point`,
    /// so the last pops first, skipping the lines `point` is outside of.
    fn push_children(&mut self, sources: Sources<'_>, fragment: FragmentRef, point: Point) {
        for_each_paint_child_on_lines(
            sources.fragments,
            fragment,
            |line| contains(line.geometry.extent(), point),
            |child| {
                self.pending.push(Visit {
                    fragment: child,
                    point,
                    expanded: false,
                });
            },
        );
    }
}

/// Walks the path from the owner of a root layer to the fragment painted on
/// top under a point: see [`PaintHitTester::cursor`].
pub struct PaintHitCursor<'a> {
    tester: &'a mut PaintHitTester,
    root: PaintLayerKey,
    /// The point in scene coordinates, then, once the path is found, in the
    /// coordinates the next fragment on the path is placed in.
    point: Point,
    /// Whether the path was searched.
    started: bool,
}

impl PaintHitCursor<'_> {
    /// The next fragment on the path, from the owner of the root layer down
    /// to the topmost fragment under the point, with the point in its local
    /// coordinates. `None` past the end, or if no fragment is under the
    /// point.
    ///
    /// The first call searches the path in the trees, which must be the
    /// same for every call: see [`PaintHitTester::hit`].
    pub fn hit_next(
        &mut self,
        layers: &PaintLayerTree,
        fragments: &FragmentTree,
        prepaint: &PrepaintTree,
    ) -> Option<HitTestResult> {
        let sources = Sources {
            layers,
            fragments,
            prepaint,
        };

        if !self.started {
            self.started = true;
            self.tester.path.clear();
            let hit = self.tester.topmost(sources, self.root, self.point)?;
            let owner = layers.layers.get(self.root)?.owner;

            // The hit fragment and its paint ancestors up to the owner, or a
            // piece of its inline element.
            let mut next = Some(hit.fragment);
            let found = loop {
                let Some(fragment) = next else {
                    break false;
                };

                self.tester.path.push(fragment);

                if layer_owner(fragments, fragment) == Some(owner) {
                    break true;
                }

                next = paint_parent(fragments, fragment);
            };

            let start = self.tester.path.last().copied();
            let point = start
                .filter(|_| found)
                .and_then(|start| sources.placement(start, self.point));

            let Some(point) = point else {
                self.tester.path.clear();
                return None;
            };
            self.point = point;
        }

        let fragment = self.tester.path.pop()?;

        let (position, next) = match fragment {
            FragmentRef::Box(key) => {
                let local = sources.local(key, self.point)?;
                // Its children are in its scrolled contents.
                (local, local + sources.scroll(key))
            }
            // The children of an inline item are placed in the same box.
            FragmentRef::Inline { .. } => (self.point - sources.location(fragment)?, self.point),
        };
        self.point = next;

        Some(HitTestResult { fragment, position })
    }
}

impl Sources<'_> {
    /// `point`, in the coordinates the box `key` is placed in, in its local
    /// coordinates. `None` if it collapses, or is not resolved.
    fn local(&self, key: BoxFragmentKey, point: Point) -> Option<Point> {
        Some(self.prepaint.fragment(key)?.inverse_transform? * point)
    }

    /// The scroll offset a box applies to its descendants: a point in its
    /// local coordinates plus the offset is in the coordinates of its
    /// scrolled contents. Zero for missing boxes.
    fn scroll(&self, key: BoxFragmentKey) -> Vec2 {
        self.fragments
            .get(key)
            .map_or(Vec2::ZERO, |fragment| self.prepaint.scroll_offset(fragment))
    }

    /// The location of an inline item in the box it is placed in.
    fn location(&self, fragment: FragmentRef) -> Option<Vec2> {
        let location = self.fragments.geometry(fragment)?.location;

        Some(Vec2::new(location.x, location.y))
    }

    /// The point at `scene`, in scene coordinates, in the scrolled contents
    /// of the box `key`, through the transforms and scroll offsets of the
    /// box and its ancestors.
    fn scene_contents(&self, key: BoxFragmentKey, scene: Point) -> Option<Point> {
        let placed = match self.fragments.boxes.parent(key) {
            Some(parent) => self.scene_contents(parent, scene)?,
            None => scene,
        };

        Some(self.local(key, placed)? + self.scroll(key))
    }

    /// The point at `scene`, in scene coordinates, in the coordinates a
    /// fragment is placed in: the scrolled contents of the parent of a box,
    /// or of the box owning the inline layout of an inline item.
    fn placement(&self, fragment: FragmentRef, scene: Point) -> Option<Point> {
        match fragment {
            FragmentRef::Box(key) => match self.fragments.boxes.parent(key) {
                Some(parent) => self.scene_contents(parent, scene),
                None => Some(scene),
            },
            FragmentRef::Inline { owner, .. } => self.scene_contents(owner, scene),
        }
    }

    /// The point in the scrolled contents of the box `key`, found from
    /// `bases`. `None` if `key` is not under a base, or a box on the way
    /// collapses or is not resolved.
    fn contents_point(&self, bases: &Bases, key: BoxFragmentKey) -> Option<Point> {
        if let Some(point) = bases.get(key) {
            return Some(point);
        }

        let parent = self.contents_point(bases, self.fragments.boxes.parent(key)?)?;

        Some(self.local(key, parent)? + self.scroll(key))
    }

    /// The piece an inline part box decorates, with the point in the
    /// coordinates the piece is placed in and in the local coordinates of
    /// the box.
    fn piece_point(
        &self,
        bases: &Bases,
        part: BoxFragmentKey,
    ) -> Option<(FragmentRef, Point, Point)> {
        let piece = piece_of_part(self.fragments, part)?;
        let parent = self.contents_point(bases, self.fragments.boxes.parent(part)?)?;

        Some((piece, parent, self.local(part, parent)?))
    }

    /// Whether a fragment starts a layer of its own, searched with its
    /// descendants, and every piece of its inline element.
    fn starts_layer(&self, fragment: FragmentRef) -> bool {
        layer_owner(self.fragments, fragment)
            .is_some_and(|owner| self.layers.layer_of(owner).is_some())
    }

    /// The fragment itself, if `point` hits it: in the local coordinates of
    /// a box, or those an inline item is placed in. Principal boxes are hit
    /// by their rounded border boxes, pieces of inline elements by those of
    /// their inline part boxes, and glyph runs by their border boxes.
    /// Anonymous boxes are never hit.
    fn hit_self(&self, fragment: FragmentRef, point: Point) -> Option<HitTestResult> {
        match fragment {
            FragmentRef::Box(key) => {
                let hit = BoxRole::of(self.fragments.get(key)?).is_principal()
                    && self.prepaint.hit_local(self.fragments, key, point);

                hit.then_some(HitTestResult {
                    fragment,
                    position: point,
                })
            }
            FragmentRef::Inline { owner, index } => {
                let position = point - self.location(fragment)?;
                let hit = match part_of_piece(self.fragments, fragment) {
                    Some(part) => self
                        .local(part, point)
                        .is_some_and(|local| self.prepaint.hit_local(self.fragments, part, local)),
                    None => self
                        .fragments
                        .inline_layout(owner)?
                        .hit_local(index as usize, point),
                };

                hit.then_some(HitTestResult { fragment, position })
            }
        }
    }

    /// Whether `point`, in the local coordinates of a box, is inside its
    /// layout overflow, which covers the border boxes of all of its
    /// descendants: outside it, none of them is hit. Its edges count as
    /// inside, so no border box edge a descendant could be hit on is left
    /// out. Missing boxes contain every point.
    fn inside_overflow(&self, key: BoxFragmentKey, point: Point) -> bool {
        self.fragments
            .get(key)
            .is_none_or(|fragment| contains(fragment.layout.geometry.overflow, point))
    }

    /// Whether `point`, in the local coordinates of a box, is inside the
    /// clip it applies to its descendants. Boxes not clipping contain every
    /// point.
    fn inside_clip(&self, key: BoxFragmentKey, point: Point) -> bool {
        let Some(fragment) = self.fragments.get(key) else {
            return true;
        };
        let (x, y) = self
            .prepaint
            .get(fragment.source)
            .map_or((false, false), |data| data.clip);

        if !BoxRole::of(fragment).is_principal() || !x && !y {
            return true;
        }

        // Clipping on both axes follows the rounded corners of the box, so
        // its clip is the area hitting it.
        if x && y {
            return self.prepaint.hit_local(self.fragments, key, point);
        }

        // Clipping on one axis stays square, unbounded on the other.
        let size = fragment.layout.geometry.size;

        (!x || (0.0..size.width).contains(&point.x))
            && (!y || (0.0..size.height).contains(&point.y))
    }
}

/// Whether `rect` contains `point`, edges included.
fn contains(rect: kurbo::Rect, point: Point) -> bool {
    (rect.x0..=rect.x1).contains(&point.x) && (rect.y0..=rect.y1).contains(&point.y)
}
