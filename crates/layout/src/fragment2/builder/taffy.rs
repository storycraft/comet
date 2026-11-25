//! Taffy trees over box fragments, laid out by the fragment tree builder.
//!
//! A Taffy node id is the FFI value of a [`BoxFragmentKey`], and each box
//! keeps its own Taffy cache. Block boxes expose their child boxes,
//! out-of-flow ones included, since Taffy hoists those to their containing
//! block. Other boxes are leaves to Taffy: their engine lays out their
//! content, and their children through [`BoxTree::root_pass`].
//!
//! The builder supplies what Taffy cannot know: the algorithm laying out each
//! box, and what follows from writing a layout.

use std::ops::{Deref, DerefMut, Range};

use slotmap::{Key, KeyData};
use taffy::{CacheTree, LayoutPartialTree, TraversePartialTree};

use super::Engine;
use crate::fragment2::{BoxFragment, BoxFragmentKey, BoxFragmentType, FragmentTree};
use crate::{
    fragment::state::NodeLayoutBits,
    style::geometry::{Point, Size},
    taffy::{conv, style::Style},
};

/// The Taffy node id of a box.
pub(super) fn node_id(key: BoxFragmentKey) -> taffy::NodeId {
    taffy::NodeId::new(key.data().as_ffi())
}

fn node_id_of(key: &BoxFragmentKey) -> taffy::NodeId {
    node_id(*key)
}

/// The box of a Taffy node id.
pub(super) fn key_of(id: taffy::NodeId) -> BoxFragmentKey {
    KeyData::from_ffi(id.into()).into()
}

/// Taffy tree for laying out one box: the box and its direct child boxes,
/// buffered for constant time indexed access.
///
/// Children live in the shared buffer of the engine. Layouts of descendants
/// push their children after this range and truncate back to it when done.
pub(super) struct BoxTree<'e, 'a> {
    engine: &'e mut Engine<'a>,
    key: BoxFragmentKey,
    /// Range of the direct children in the shared buffer.
    children: Range<usize>,
}

impl<'e, 'a> BoxTree<'e, 'a> {
    /// The tree laying out `key`. Block boxes expose their child boxes.
    /// Other boxes are leaves to Taffy.
    pub(super) fn new(engine: &'e mut Engine<'a>, key: BoxFragmentKey) -> Self {
        let (fragments, buffer) = (&*engine.fragments, &mut engine.scratch.children);
        let start = buffer.len();

        if fragments
            .get(key)
            .is_some_and(|fragment| fragment.child_layout.is_block_container())
        {
            buffer.extend(fragments.boxes.children(key));
        }

        let end = buffer.len();

        Self {
            engine,
            key,
            children: start..end,
        }
    }

    /// The tree for Taffy's root pass over `key`, or for laying out its
    /// children with `compute_child_layout`, without buffering them: neither
    /// traverses its children here.
    pub(super) fn root_pass(engine: &'e mut Engine<'a>, key: BoxFragmentKey) -> Self {
        let start = engine.scratch.children.len();

        Self {
            engine,
            key,
            children: start..start,
        }
    }

    fn children_of(&self, id: taffy::NodeId) -> &[BoxFragmentKey] {
        debug_assert_eq!(key_of(id), self.key, "only the laid out box is traversed");

        &self.engine.scratch.children[self.children.clone()]
    }

    fn fragment(&self, id: taffy::NodeId) -> &BoxFragment {
        &self.engine.fragments.boxes[key_of(id)]
    }
}

impl Drop for BoxTree<'_, '_> {
    fn drop(&mut self) {
        self.engine.scratch.children.truncate(self.children.start);
    }
}

impl<'a> Deref for BoxTree<'_, 'a> {
    type Target = Engine<'a>;

    fn deref(&self) -> &Engine<'a> {
        self.engine
    }
}

impl<'a> DerefMut for BoxTree<'_, 'a> {
    fn deref_mut(&mut self) -> &mut Engine<'a> {
        self.engine
    }
}

impl TraversePartialTree for BoxTree<'_, '_> {
    type ChildIter<'a>
        = std::iter::Map<std::slice::Iter<'a, BoxFragmentKey>, fn(&BoxFragmentKey) -> taffy::NodeId>
    where
        Self: 'a;

    fn child_ids(&self, id: taffy::NodeId) -> Self::ChildIter<'_> {
        self.children_of(id).iter().map(node_id_of)
    }

    fn child_count(&self, id: taffy::NodeId) -> usize {
        self.children_of(id).len()
    }

    fn get_child_id(&self, id: taffy::NodeId, index: usize) -> taffy::NodeId {
        node_id(self.children_of(id)[index])
    }
}

impl LayoutPartialTree for BoxTree<'_, '_> {
    type CoreContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type CustomIdent = String;

    fn get_core_container_style(&self, id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        &self.fragment(id).style
    }

    fn set_unrounded_layout(&mut self, id: taffy::NodeId, layout: &taffy::Layout) {
        self.engine.set_unrounded_layout(key_of(id), layout);
    }

    fn compute_child_layout(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        taffy::compute_cached_layout(self, id, input, |tree, id, input| {
            tree.engine.layout_box(key_of(id), input, None)
        })
    }
}

impl taffy::LayoutBlockContainer for BoxTree<'_, '_> {
    type BlockContainerStyle<'a>
        = &'a Style
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_block_container_style(&self, id: taffy::NodeId) -> Self::BlockContainerStyle<'_> {
        self.get_core_container_style(id)
    }

    fn get_block_child_style(&self, id: taffy::NodeId) -> Self::BlockItemStyle<'_> {
        self.get_core_container_style(id)
    }

    fn compute_block_child_layout(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        // Flow roots start a block formatting context of their own.
        let flow_root = self.fragment(id).box_type == BoxFragmentType::FlowRoot;
        let block_ctx = block_ctx.filter(|_| !flow_root);

        taffy::compute_cached_layout(self, id, input, |tree, id, input| {
            tree.engine.layout_box(key_of(id), input, block_ctx)
        })
    }
}

impl taffy::LayoutContainingBlock for BoxTree<'_, '_> {
    type OofItemStyle<'a>
        = &'a Style
    where
        Self: 'a;

    fn get_oof_item_style(&self, id: taffy::NodeId) -> Self::OofItemStyle<'_> {
        self.get_core_container_style(id)
    }

    fn clear_hoisted_children(&mut self, id: taffy::NodeId) {
        if let Some(fragment) = self.engine.fragments.get_mut(key_of(id)) {
            fragment.hoisted.clear();
        }
    }

    fn add_hoisted_children(&mut self, id: taffy::NodeId, children: &[taffy::NodeId]) {
        if let Some(fragment) = self.engine.fragments.get_mut(key_of(id)) {
            fragment
                .hoisted
                .extend(children.iter().map(|&child| key_of(child)));
        }
    }
}

impl CacheTree for BoxTree<'_, '_> {
    fn cache_get(
        &mut self,
        id: taffy::NodeId,
        input: &taffy::LayoutInput,
    ) -> Option<taffy::LayoutOutput> {
        let fragment = self.engine.fragments.get_mut(key_of(id))?;

        if input.run_mode == taffy::RunMode::PerformLayout && !fragment.final_layout_valid {
            return None;
        }

        fragment.cache.get(&cache_key(fragment, input))
    }

    fn cache_store(
        &mut self,
        id: taffy::NodeId,
        input: &taffy::LayoutInput,
        output: taffy::LayoutOutput,
    ) {
        let Some(fragment) = self.engine.fragments.get_mut(key_of(id)) else {
            return;
        };
        fragment.cache.store(&cache_key(fragment, input), output);

        if input.run_mode == taffy::RunMode::PerformLayout {
            fragment.final_layout_valid = true;
        }
    }

    fn cache_clear(&mut self, id: taffy::NodeId) {
        if let Some(fragment) = self.engine.fragments.get_mut(key_of(id)) {
            fragment.cache.clear();
        }
    }
}

/// The cache key of a layout input. A box with both dimensions known and no
/// lengths resolving against its parent lays out the same in any available
/// space, so the available space and the parent size are left out.
fn cache_key(fragment: &BoxFragment, input: &taffy::LayoutInput) -> taffy::LayoutInput {
    match input.known_dimensions {
        taffy::Size {
            width: Some(width),
            height: Some(height),
        } if !fragment.bits.contains(NodeLayoutBits::DEPENDS_PARENT_SIZE) => taffy::LayoutInput {
            available_space: taffy::Size {
                width: taffy::AvailableSpace::Definite(width),
                height: taffy::AvailableSpace::Definite(height),
            },
            parent_size: taffy::Size::NONE,
            ..*input
        },
        _ => *input,
    }
}

/// Write a Taffy layout into the unrounded layout and geometry of the box
/// `key`. Returns whether the box moved, resized or its overflow changed.
///
/// In-flow boxes inside inline content are placed by the lines of their
/// owner, which writes their locations itself, so their locations are kept.
fn write_layout(fragments: &mut FragmentTree, key: BoxFragmentKey, layout: &taffy::Layout) -> bool {
    let placed_by_lines = fragments
        .get(key)
        .is_some_and(|fragment| fragment.box_type != BoxFragmentType::OutOfFlow)
        && fragments
            .boxes
            .parent(key)
            .is_some_and(|parent| fragments.boxes[parent].child_layout.is_inline());
    let Some(fragment) = fragments.get_mut(key) else {
        return false;
    };
    fragment.layout.unrounded = *layout;

    let location = if placed_by_lines {
        fragment.layout.geometry.location
    } else {
        Point::new(f64::from(layout.location.x), f64::from(layout.location.y))
    };
    let size = Size::new(f64::from(layout.size.width), f64::from(layout.size.height));
    let overflow = overflow(layout);
    let geometry = &mut fragment.layout.geometry;
    let changed =
        geometry.location != location || geometry.size != size || geometry.overflow != overflow;

    geometry.location = location;
    geometry.size = size;
    geometry.padding = conv::from_rect(layout.padding, f64::from);
    geometry.border = conv::from_rect(layout.border, f64::from);
    geometry.overflow = overflow;

    changed
}

/// The layout overflow of a box laid out by Taffy, in its local coordinates:
/// its border box, with Taffy's scrollable overflow rectangle moved from the
/// top-left corner of the padding box to the border box.
fn overflow(layout: &taffy::Layout) -> kurbo::Rect {
    let rect = layout.scrollable_overflow_rect;
    let (x, y) = (f64::from(layout.border.left), f64::from(layout.border.top));
    let content = kurbo::Rect::new(
        x + f64::from(rect.left),
        y + f64::from(rect.top),
        x + f64::from(rect.right),
        y + f64::from(rect.bottom),
    );
    let border_box = kurbo::Rect::new(
        0.0,
        0.0,
        f64::from(layout.size.width),
        f64::from(layout.size.height),
    );

    border_box.union(content)
}

/// `content`, an area in the local coordinates of a box with `border`, as a
/// Taffy scrollable overflow rectangle measured from the top-left corner of
/// its padding box. The inverse of [`overflow`] for the content.
pub(super) fn scrollable_overflow(
    content: kurbo::Rect,
    border: taffy::Rect<f32>,
) -> taffy::Rect<f32> {
    let (x, y) = (f64::from(border.left), f64::from(border.top));

    taffy::Rect {
        left: (content.x0 - x) as f32,
        right: (content.x1 - x) as f32,
        top: (content.y0 - y) as f32,
        bottom: (content.y1 - y) as f32,
    }
}

impl Engine<'_> {
    /// Write a Taffy layout into the box `key`, marking its source
    /// [`PAINT_CHANGED`](super::PAINT_CHANGED) when it moved, resized or its
    /// overflow changed.
    pub(super) fn set_unrounded_layout(&mut self, key: BoxFragmentKey, layout: &taffy::Layout) {
        if write_layout(self.fragments, key, layout) {
            let source = self.fragments.boxes[key].source;
            self.mark_paint(source);
        }
    }
}
