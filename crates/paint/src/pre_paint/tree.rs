use std::ops::Index;

use comet_layout::{
    fragment::{FragmentKey, tree::FragmentTree},
    input::LayoutInputKey,
};
use kurbo::{Point, Rect, Shape};
use slotmap::SecondaryMap;

use crate::pre_paint::{FragmentData, LayerFlags, LayoutInputData, PrePaintFlags};

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
    pub(super) fragment_generation: u64,
    /// Generation of the latest resolve, increasing with every resolve.
    pub(super) generation: u64,
    /// Invalidation state of fragments. Fragments without an entry are clean.
    pub(super) flags: SecondaryMap<FragmentKey, PrePaintFlags>,
    /// Layer changes of fragments. Fragments without an entry are unchanged.
    pub(super) layer_flags: SecondaryMap<FragmentKey, LayerFlags>,
    /// [`Self::generation`] when the tree was last cleared, or 0.
    pub(super) cleared_generation: u64,
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

    /// Generation of the latest
    /// [`resolve`](super::resolver::PrePaintTreeResolver::resolve). Fragment
    /// data it created or changed has it as its
    /// [`generation`](FragmentData::generation), and so do the
    /// [`subtree_generation`](FragmentData::subtree_generation) of their
    /// ancestors. Only increases, also across [`Self::clear`].
    pub fn generation(&self) -> u64 {
        self.generation
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

    /// The layer changes of a fragment, set by resolves since the last
    /// [`Self::clear_layer_flags`].
    pub fn layer_flags(&self, key: FragmentKey) -> LayerFlags {
        self.layer_flags.get(key).copied().unwrap_or_default()
    }

    /// Clear the layer flags of every fragment. Layer builds keep them, so
    /// run it after building layers.
    pub fn clear_layer_flags(&mut self) {
        self.layer_flags.clear();
    }

    /// [`Self::generation`] when the tree was last [cleared](Self::clear), or
    /// 0. Layer flags do not cover what changed before, so layers built at
    /// or before it are built again.
    pub fn cleared_generation(&self) -> u64 {
        self.cleared_generation
    }

    /// Set the layer change of a fragment, [`LayerFlags::NEED_LAYER`] or
    /// [`LayerFlags::NEED_NO_LAYER`], replacing the other, and mark its
    /// ancestors [`LayerFlags::CHILD_LAYER_UPDATE`].
    pub(super) fn mark_layer(
        &mut self,
        fragments: &FragmentTree,
        key: FragmentKey,
        change: LayerFlags,
    ) {
        let Some(entry) = self.layer_flags.entry(key) else {
            return;
        };
        let flags = entry.or_default();
        flags.remove(LayerFlags::NEED_LAYER | LayerFlags::NEED_NO_LAYER);
        flags.insert(change);

        let mut ancestor = fragments.items.parent(key);

        while let Some(key) = ancestor {
            let Some(entry) = self.layer_flags.entry(key) else {
                break;
            };
            let flags = entry.or_default();

            // Ancestors of a marked fragment are marked already.
            if flags.contains(LayerFlags::CHILD_LAYER_UPDATE) {
                break;
            }

            flags.insert(LayerFlags::CHILD_LAYER_UPDATE);
            ancestor = fragments.items.parent(key);
        }
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
        self.layer_flags.clear();
        // The generation keeps increasing, so data resolved again is newer
        // than anything consumers have seen.
        self.fragment_generation = 0;
        self.cleared_generation = self.generation;
    }

    /// Whether `point`, in the local coordinates of a fragment, hits the
    /// fragment itself: it falls in its border box, rounded with its
    /// [border radius](FragmentData::border_radius).
    ///
    /// Fragments not in `fragments`, or not resolved in `self`, are never
    /// hit.
    pub fn hit_local(&self, fragments: &FragmentTree, key: FragmentKey, point: Point) -> bool {
        let Some(fragment) = fragments.items.get(key) else {
            return false;
        };

        let Some(data) = self.fragment(key) else {
            return false;
        };

        let size = fragment.layout.size;
        let rect = Rect::from_origin_size(Point::ORIGIN, (size.width, size.height));
        let Some(radii) = data.border_radius else {
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
