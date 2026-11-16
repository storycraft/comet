use std::ops::Index;

use comet_layout::{
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
    input::LayoutInputKey,
};
use kurbo::{Point, Rect, RoundedRectRadii, Shape};
use slotmap::SecondaryMap;

use crate::pre_paint::{FragmentData, LayoutInputData, PrePaintFlags};

/// Paint data of the fragments of a fragment tree, such as their transforms
/// relative to their parents, and of the layout input items they were built
/// from.
///
/// Filled by a [`PrePaintTreeResolver`](super::resolver::PrePaintTreeResolver),
/// which resolves the fragments fragment builds changed and the fragments
/// [marked](Self::mark) with [`PrePaintFlags::NEED_PRE_PAINT`].
/// Keys are generational, so data left behind by deleted fragments and items
/// is never returned for the fragments and items reusing their slots, and is
/// overwritten when those are resolved.
#[derive(Default)]
pub struct PrePaintTree {
    pub(super) fragment_mapping: SecondaryMap<FragmentKey, FragmentData>,
    pub(super) layout_mapping: SecondaryMap<LayoutInputKey, LayoutInputData>,
    /// [Generation](FragmentTree::generation) of the fragment tree when
    /// [`resolve`](super::resolver::PrePaintTreeResolver::resolve) last
    /// succeeded, or 0.
    pub(super) generation: u64,
    /// Invalidation state of fragments. Fragments without an entry are clean.
    pub(super) flags: SecondaryMap<FragmentKey, PrePaintFlags>,
}

impl PrePaintTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// The paint data a fragment was resolved to.
    pub fn fragment(&self, key: FragmentKey) -> Option<&FragmentData> {
        self.fragment_mapping.get(key)
    }

    /// The paint data a layout input item was resolved to.
    pub fn get(&self, key: LayoutInputKey) -> Option<&LayoutInputData> {
        self.layout_mapping.get(key)
    }

    /// The invalidation state of a fragment.
    pub fn flags(&self, key: FragmentKey) -> PrePaintFlags {
        self.flags.get(key).copied().unwrap_or_default()
    }

    /// Mark a fragment [`PrePaintFlags::NEED_PRE_PAINT`] and its ancestors
    /// [`PrePaintFlags::CHILD_NEED_PRE_PAINT`], so the next
    /// [`resolve`](super::resolver::PrePaintTreeResolver::resolve) resolves it
    /// and its item again, such as after their paint styles changed.
    /// Fragments not in `fragments` are ignored.
    ///
    /// Mark after the fragment build: line, inline and glyph run fragments
    /// are replaced when their lines are laid out again, and fragments moved
    /// by a build keep marks propagated to their old ancestors only.
    pub fn mark(&mut self, fragments: &FragmentTree, key: FragmentKey) {
        if fragments.items.get(key).is_none() {
            return;
        }

        self.insert_flags(key, PrePaintFlags::NEED_PRE_PAINT);

        let mut ancestor = fragments.items.parent(key);

        while let Some(key) = ancestor {
            // Ancestors of a marked fragment are marked already.
            if self
                .flags(key)
                .contains(PrePaintFlags::CHILD_NEED_PRE_PAINT)
            {
                break;
            }

            self.insert_flags(key, PrePaintFlags::CHILD_NEED_PRE_PAINT);
            ancestor = fragments.items.parent(key);
        }
    }

    /// Clear the flags of every fragment. The resolver keeps them, so run it
    /// after resolving.
    pub fn clear_flags(&mut self) {
        self.flags.clear();
    }

    fn insert_flags(&mut self, key: FragmentKey, flags: PrePaintFlags) {
        // Only keys removed from their slot map have no entry.
        if let Some(entry) = self.flags.entry(key) {
            entry.or_default().insert(flags);
        }
    }

    pub fn clear(&mut self) {
        self.fragment_mapping.clear();
        self.layout_mapping.clear();
        self.flags.clear();
        self.generation = 0;
    }

    /// Corner radii of a fragment's border box: the
    /// [border radius](LayoutInputData::border_radius) of its item, with the
    /// corners joining a linked fragment square. Box fragments continue below
    /// their previous fragment, inline and glyph run fragments to its right.
    ///
    /// `None` for fragments painting no box, such as line boxes and anonymous
    /// blocks, items setting no radius, and fragments or items missing from
    /// `fragments` or `self`.
    pub fn border_radius(
        &self,
        fragments: &FragmentTree,
        key: FragmentKey,
    ) -> Option<RoundedRectRadii> {
        let fragment = fragments.items.get(key)?;

        if matches!(
            fragment.item,
            FragmentItem::Line { .. } | FragmentItem::AnonymousBlock { .. }
        ) {
            return None;
        }

        let radii = self.get(fragment.source)?.border_radius?;

        Some(split_radii(fragment, radii))
    }

    /// Whether `point`, in the local coordinates of a fragment, hits the
    /// fragment itself: it falls in its border box, rounded with its
    /// [border radius](Self::border_radius).
    ///
    /// Fragments not in `fragments`, or not resolved in `self`, are never
    /// hit.
    pub fn hit_local(&self, fragments: &FragmentTree, key: FragmentKey, point: Point) -> bool {
        let Some(fragment) = fragments.items.get(key) else {
            return false;
        };

        if self.fragment(key).is_none() {
            return false;
        }

        let size = fragment.layout.size;
        let rect = Rect::from_origin_size(Point::ORIGIN, (size.width, size.height));
        let Some(radii) = self.border_radius(fragments, key) else {
            return rect.contains(point);
        };

        rect.to_rounded_rect(radii).contains(point)
    }
}

impl Index<FragmentKey> for PrePaintTree {
    type Output = FragmentData;

    fn index(&self, key: FragmentKey) -> &FragmentData {
        &self.fragment_mapping[key]
    }
}

impl Index<LayoutInputKey> for PrePaintTree {
    type Output = LayoutInputData;

    fn index(&self, key: LayoutInputKey) -> &LayoutInputData {
        &self.layout_mapping[key]
    }
}

/// The corner radii of a fragment of an item with `radii`, with the corners
/// joining a linked fragment square.
fn split_radii(fragment: &Fragment, mut radii: RoundedRectRadii) -> RoundedRectRadii {
    // Box fragments continue below their previous fragment, inline and glyph
    // run fragments to its right.
    let vertical = fragment.item.is_box();

    if fragment.link.prev.is_some() {
        radii.top_left = 0.0;

        if vertical {
            radii.top_right = 0.0;
        } else {
            radii.bottom_left = 0.0;
        }
    }

    if fragment.link.next.is_some() {
        radii.bottom_right = 0.0;

        if vertical {
            radii.bottom_left = 0.0;
        } else {
            radii.top_right = 0.0;
        }
    }

    radii
}
