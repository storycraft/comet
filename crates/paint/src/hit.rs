//! Hit testing of painted fragments, finding the fragment under a point in
//! the order fragments paint in.

use comet_layout::fragment::{FragmentItem, FragmentKey, tree::FragmentTree};
use kurbo::Point;
use slotmap::SecondaryMap;

use crate::{
    layer::{PaintLayerEntry, PaintLayerKey, tree::PaintLayerTree},
    pre_paint::tree::PrePaintTree,
};

/// Finds the fragment painted on top under a point. Keep it to reuse its
/// buffers across hit tests.
#[derive(Default)]
pub struct PaintHitTester {
    /// The point in the local coordinates of each fragment looked at so far,
    /// or `None` for fragments it cannot hit.
    locals: SecondaryMap<FragmentKey, Option<Local>>,
    /// Layer entries to search, popped in reverse paint order.
    pending: Vec<PaintLayerEntry>,
}

/// The point hit, in the local coordinates of a fragment.
#[derive(Clone, Copy)]
struct Local {
    point: Point,
    /// Whether the point lies outside a clip of the fragment's ancestors.
    clipped: bool,
}

impl PaintHitTester {
    pub fn new() -> Self {
        Self::default()
    }

    /// The topmost fragment under `point`, in scene coordinates, among the
    /// layers of `root` in `layers`, or `None` if no fragment is under it.
    ///
    /// Entries are searched depth first in reverse paint order, so fragments
    /// painting on top are hit first: later entries before earlier ones, and
    /// a child layer's entries before the entries painting below it. A
    /// fragment is hit when `point` falls in its border box, rounded with its
    /// border radius, after mapping it through the transforms of the fragment
    /// and its ancestors, including the ancestors of `root`'s owner. See
    /// [`PrePaintTree::hit_local`].
    ///
    /// Points outside the clip of a box clipping its overflow do not hit its
    /// descendants, like the painter. Clips of the ancestors of `root`'s
    /// owner do not apply. Line boxes and anonymous blocks are never hit, and
    /// neither are fragments whose transforms collapse them to no area.
    ///
    /// `pre_paint` must be resolved for `fragments`, and `layers` built for
    /// them. Returns `None` if `root` is not in `layers`. Fragments missing
    /// from `fragments` or `pre_paint` are never hit, and neither are their
    /// descendants.
    pub fn hit(
        &mut self,
        layers: &PaintLayerTree,
        fragments: &FragmentTree,
        pre_paint: &PrePaintTree,
        root: PaintLayerKey,
        point: Point,
    ) -> Option<FragmentKey> {
        let root_owner = layers.get(root)?.owner;

        self.locals.clear();
        self.pending.clear();

        let mut hit = Hit {
            fragments,
            pre_paint,
            root: root_owner,
            point,
            locals: &mut self.locals,
        };

        // Entries are pushed in paint order, so they pop in reverse.
        self.pending.push(PaintLayerEntry::Layer(root));

        while let Some(entry) = self.pending.pop() {
            match entry {
                PaintLayerEntry::Fragment(key) => {
                    if hit.hits(key) {
                        return Some(key);
                    }
                }
                PaintLayerEntry::Layer(layer) => {
                    if let Some(layer) = layers.get(layer) {
                        self.pending.extend_from_slice(&layer.entries);
                    }
                }
            }
        }

        None
    }
}

/// State of one [`PaintHitTester::hit`].
struct Hit<'a> {
    fragments: &'a FragmentTree,
    pre_paint: &'a PrePaintTree,
    /// Owner of the root layer. Clips of its ancestors do not apply.
    root: FragmentKey,
    /// The point hit, in scene coordinates.
    point: Point,
    locals: &'a mut SecondaryMap<FragmentKey, Option<Local>>,
}

impl Hit<'_> {
    /// Whether the point hits a fragment itself, inside the clips of its
    /// ancestors.
    fn hits(&mut self, key: FragmentKey) -> bool {
        // Line boxes and anonymous blocks are never hit: skip mapping the
        // point for them.
        if self.fragments.items.get(key).is_none_or(|fragment| {
            matches!(
                fragment.item,
                FragmentItem::Line { .. } | FragmentItem::AnonymousBlock { .. }
            )
        }) {
            return false;
        }

        let Some(local) = self.local(key) else {
            return false;
        };

        !local.clipped && self.pre_paint.hit_local(self.fragments, key, local.point)
    }

    /// The point in the local coordinates of a fragment, mapped from its
    /// parent's.
    fn local(&mut self, key: FragmentKey) -> Option<Local> {
        if let Some(&local) = self.locals.get(key) {
            return local;
        }

        let parent = self.fragments.items.parent(key);
        let (point, clipped) = match parent {
            Some(parent) => {
                let local = self.local(parent)?;
                let clipped =
                    key != self.root && (local.clipped || !self.inside_clip(parent, local.point));

                (local.point, clipped)
            }
            None => (self.point, false),
        };
        // Unresolved fragments are not hit, like collapsed ones.
        let local = self
            .pre_paint
            .fragment(key)
            .and_then(|data| data.inverse_transform)
            .map(|inverse| Local {
                point: inverse * point,
                clipped,
            });

        self.locals.insert(key, local);
        local
    }

    /// Whether `point`, in the local coordinates of a fragment, is inside the
    /// clip it applies to its descendants. Fragments not clipping contain
    /// every point.
    fn inside_clip(&self, key: FragmentKey, point: Point) -> bool {
        // A fragment without data clips nothing.
        let Some(fragment) = self.fragments.items.get(key) else {
            return true;
        };
        let (x, y) = self
            .pre_paint
            .get(fragment.source)
            .map_or((false, false), |data| data.clip);

        if !fragment.item.is_box() || !x && !y {
            return true;
        }

        // Clipping on both axes follows the rounded corners of the box, so
        // its clip is the area hitting it.
        if x && y {
            return self.pre_paint.hit_local(self.fragments, key, point);
        }

        // Clipping on one axis stays square, unbounded on the other.
        let size = fragment.layout.size;

        (!x || (0.0..size.width).contains(&point.x))
            && (!y || (0.0..size.height).contains(&point.y))
    }
}
