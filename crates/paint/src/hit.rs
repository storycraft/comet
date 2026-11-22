//! Hit testing of painted fragments, finding the fragment under a point in
//! the order fragments paint in.

use comet_layout::fragment::{FragmentItem, FragmentKey, tree::FragmentTree};
use kurbo::Point;

use crate::{
    layer::{PaintLayerKey, tree::PaintLayerTree},
    prepaint::tree::PrepaintTree,
};

/// The fragment found under a point by [`PaintHitTester::hit`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HitTestResult {
    /// The fragment hit.
    pub fragment: FragmentKey,
    /// The point in the local coordinates of [`Self::fragment`], with its
    /// border box's top-left corner at the origin, after mapping it through
    /// the transforms of the fragment and its ancestors.
    pub position: Point,
}

/// Finds the fragment painted on top under a point. Keep it to reuse its
/// buffers across hit tests.
#[derive(Default)]
pub struct PaintHitTester {
    /// Fragments to test in the layer being searched.
    pending: Vec<Visit>,
    /// Ancestors of a fragment, innermost first.
    chain: Vec<FragmentKey>,
}

/// A step of searching the fragments of a layer, in reverse paint order.
#[derive(Clone, Copy)]
struct Visit {
    key: FragmentKey,
    /// The point in the local coordinates of the fragment's parent, or of
    /// the fragment itself once its children were queued.
    point: Point,
    /// Whether the fragment's children were queued, so it is tested next.
    expanded: bool,
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
    /// or `None` if no fragment is under it.
    ///
    /// Fragments are searched in reverse paint order, so fragments painting
    /// on top are hit first: in each layer, its child layers with a
    /// non-negative z index from the last, then its other fragments from the
    /// last painted, children before their parents, then its child layers
    /// with a negative z index from the last, then its owner. A fragment is
    /// hit when `point` falls in its border box, rounded with its border
    /// radius, after mapping it through the transforms of the fragment and
    /// its ancestors, including the ancestors of `root`'s owner. See
    /// [`PrepaintTree::hit_local`].
    ///
    /// Points outside the clip of a box clipping its overflow do not hit its
    /// descendants, like the painter, and their subtrees are not searched.
    /// Neither are the subtrees of fragments whose
    /// [overflow](comet_layout::fragment::FragmentBox::overflow) does not
    /// contain the point, since it covers the border boxes of every
    /// descendant. Child layers are searched on their own.
    /// Clips of the ancestors of `root`'s owner do not apply. Line boxes and
    /// anonymous blocks are never hit, and neither are fragments whose
    /// transforms collapse them to no area.
    ///
    /// `prepaint` must be resolved for `fragments`, and `layers` built for
    /// them. Returns `None` if `root` is not in `layers`. Fragments missing
    /// from `fragments` or `prepaint` are never hit, and neither are their
    /// descendants.
    pub fn hit(
        &mut self,
        layers: &PaintLayerTree,
        fragments: &FragmentTree,
        prepaint: &PrepaintTree,
        root: PaintLayerKey,
        point: Point,
    ) -> Option<HitTestResult> {
        let sources = Sources {
            layers,
            fragments,
            prepaint,
        };
        let owner = layers.layers.get(root)?.owner;

        // Map the point to the local coordinates of the owner's parent.
        self.chain.clear();
        let mut next = fragments.items.parent(owner);

        while let Some(fragment) = next {
            self.chain.push(fragment);
            next = fragments.items.parent(fragment);
        }

        let mut point = point;

        for &key in self.chain.iter().rev() {
            point = prepaint.fragment(key)?.inverse_transform? * point;
        }

        self.pending.clear();
        self.hit_layer(sources, root, point)
    }

    /// The topmost fragment of a layer under `point`, in the local
    /// coordinates of its owner's parent.
    fn hit_layer(
        &mut self,
        sources: Sources<'_>,
        key: PaintLayerKey,
        point: Point,
    ) -> Option<HitTestResult> {
        let owner = sources.layers.layers.get(key)?.owner;
        let point = sources.prepaint.fragment(owner)?.inverse_transform? * point;
        // The owner's clip bounds its descendants, child layers included.
        let inside = inside_clip(sources, owner, point);

        if inside {
            // The last child layer with a negative z index, where the search
            // of negative layers resumes after the contents.
            let mut negative = None;

            for child in sources.layers.layers.children(key).rev() {
                if sources.layers.layers[child].z_index < 0 {
                    negative = Some(child);
                    break;
                }

                if let Some(hit) = self.hit_child(sources, child, owner, point) {
                    return Some(hit);
                }
            }

            if inside_overflow(sources, owner, point)
                && let Some(hit) = self.hit_contents(sources, owner, point)
            {
                return Some(hit);
            }

            while let Some(child) = negative {
                if let Some(hit) = self.hit_child(sources, child, owner, point) {
                    return Some(hit);
                }

                negative = sources.layers.layers.prev_sibling(child);
            }
        }

        sources
            .prepaint
            .hit_local(sources.fragments, owner, point)
            .then_some(HitTestResult {
                fragment: owner,
                position: point,
            })
    }

    /// The topmost fragment of a child layer of the layer owned by `owner`
    /// under `point`, in the local coordinates of `owner`, inside the clips
    /// of the fragments between their owners.
    fn hit_child(
        &mut self,
        sources: Sources<'_>,
        child: PaintLayerKey,
        owner: FragmentKey,
        point: Point,
    ) -> Option<HitTestResult> {
        let child_owner = sources.layers.layers.get(child)?.owner;

        // Fragments between the owners, innermost first.
        self.chain.clear();
        let mut next = sources.fragments.items.parent(child_owner);

        while let Some(fragment) = next
            && fragment != owner
        {
            self.chain.push(fragment);
            next = sources.fragments.items.parent(fragment);
        }

        // An outdated layer not under the owner is not hit.
        next?;

        let mut point = point;

        for &key in self.chain.iter().rev() {
            point = sources.prepaint.fragment(key)?.inverse_transform? * point;

            if !inside_clip(sources, key, point) {
                return None;
            }
        }

        self.hit_layer(sources, child, point)
    }

    /// The topmost descendant of a layer's owner under `point`, in the local
    /// coordinates of the owner, skipping the owners of child layers and
    /// their descendants.
    fn hit_contents(
        &mut self,
        sources: Sources<'_>,
        owner: FragmentKey,
        point: Point,
    ) -> Option<HitTestResult> {
        let base = self.pending.len();

        // Children pop from the last, each after its own children.
        self.pending
            .extend(sources.fragments.items.children(owner).map(|child| Visit {
                key: child,
                point,
                expanded: false,
            }));

        while self.pending.len() > base {
            let Visit {
                key,
                point,
                expanded,
            } = self.pending.pop()?;

            if expanded {
                let Some(fragment) = sources.fragments.items.get(key) else {
                    continue;
                };

                if !matches!(
                    fragment.item,
                    FragmentItem::Line { .. } | FragmentItem::AnonymousBlock { .. }
                ) && sources.prepaint.hit_local(sources.fragments, key, point)
                {
                    self.pending.truncate(base);
                    return Some(HitTestResult {
                        fragment: key,
                        position: point,
                    });
                }

                continue;
            }

            // Child layers are searched on their own, and collapsed or
            // unresolved fragments are not hit with their descendants.
            if sources.layers.layer_of(key).is_some() {
                continue;
            }

            let Some(point) = sources
                .prepaint
                .fragment(key)
                .and_then(|data| data.inverse_transform)
                .map(|inverse| inverse * point)
            else {
                continue;
            };

            self.pending.push(Visit {
                key,
                point,
                expanded: true,
            });

            if inside_overflow(sources, key, point) && inside_clip(sources, key, point) {
                self.pending
                    .extend(sources.fragments.items.children(key).map(|child| Visit {
                        key: child,
                        point,
                        expanded: false,
                    }));
            }
        }

        None
    }
}

/// Whether `point`, in the local coordinates of a fragment, is inside its
/// layout overflow, which covers the border boxes of all of its descendants:
/// outside it, none of them is hit. Its edges count as inside, so no border
/// box edge a descendant could be hit on is left out. Missing fragments
/// contain every point.
fn inside_overflow(sources: Sources<'_>, key: FragmentKey, point: Point) -> bool {
    sources.fragments.items.get(key).is_none_or(|fragment| {
        let overflow = fragment.layout.overflow;

        (overflow.x0..=overflow.x1).contains(&point.x)
            && (overflow.y0..=overflow.y1).contains(&point.y)
    })
}

/// Whether `point`, in the local coordinates of a fragment, is inside the
/// clip it applies to its descendants. Fragments not clipping contain every
/// point.
fn inside_clip(sources: Sources<'_>, key: FragmentKey, point: Point) -> bool {
    let Some(fragment) = sources.fragments.items.get(key) else {
        return true;
    };
    let (x, y) = sources
        .prepaint
        .get(fragment.source)
        .map_or((false, false), |data| data.clip);

    if !fragment.item.is_box() || !x && !y {
        return true;
    }

    // Clipping on both axes follows the rounded corners of the box, so its
    // clip is the area hitting it.
    if x && y {
        return sources.prepaint.hit_local(sources.fragments, key, point);
    }

    // Clipping on one axis stays square, unbounded on the other.
    let size = fragment.layout.size;

    (!x || (0.0..size.width).contains(&point.x)) && (!y || (0.0..size.height).contains(&point.y))
}
