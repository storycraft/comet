//! Step 2 of a build: drop the cached layouts that box changes affect.
//!
//! Items marked [`DirtyFlags::NEED_BOX_LAYOUT`] are visited along their
//! ancestors' child bits, and after a viewport change, boxes depending on it
//! are visited along the [propagated](NodeLayoutBits::PROPAGATED) bits. Nothing
//! is laid out:
//! - A box resolves its style again, updates its type within its flow, and
//!   drops the cached layouts of itself and its ancestors.
//! - An inline element or text drops those of the box whose inline layout
//!   holds it, which lays that layout out again without shaping.
//!
//! Boxes created by the child sync are resolved already. Moving a box inside
//! inline content in or out of flow is marked
//! [`DirtyFlags::NEED_FULL_LAYOUT`] by the caller, so the child sync built
//! that inline layout again.

use super::{Engine, classify};
use crate::{
    dirty_map::DirtyFlags,
    fragment::state::NodeLayoutBits,
    fragment2::{BoxFragmentKey, BoxFragmentType},
    input::LayoutInputKey,
};

impl Engine<'_> {
    /// Invalidate the boxes of the build root affected by box changes.
    pub(super) fn invalidate(&mut self) {
        let mut stack = std::mem::take(&mut self.scratch.walk);
        stack.clear();

        if let Some(root_box) = self.root_box {
            stack.push((self.root, root_box));
        }

        // Each item comes with the box of its nearest ancestor generating
        // one, or its own box.
        while let Some((item, owner)) = stack.pop() {
            let tree = self.cx.tree;
            if tree.items.get(item).is_none() {
                continue;
            }

            let entry = self.dirty.entry(item);
            let own = self.fragments.box_fragment(item);
            let viewport = self.constraint_viewport_changed()
                && own.is_some_and(|key| {
                    self.fragments.boxes[key]
                        .bits
                        .contains(NodeLayoutBits::DEPENDS_VIEWPORT)
                });

            if entry.flags.contains(DirtyFlags::NEED_BOX_LAYOUT) || viewport {
                match own {
                    Some(key) => self.restyle(item, key),
                    None => self.invalidate_up(owner),
                }
            }

            let owner = own.unwrap_or(owner);
            if self.fragments.boxes[owner].child_layout.is_content() {
                continue;
            }

            let descend = entry.child_bits.contains(DirtyFlags::NEED_BOX_LAYOUT);
            if descend {
                stack.extend(tree.items.children(item).map(|child| (child, owner)));
            }

            if viewport {
                // Inline elements carry no bits, so the boxes inside inline
                // content are reached from their owner. Those not depending
                // on the viewport are skipped when visited.
                if let Some(inline) = self.fragments.boxes[owner].inline() {
                    stack.extend(
                        inline
                            .atomics
                            .keys()
                            .filter(|&&child| !descend || tree.items.parent(child) != Some(item))
                            .map(|&child| (child, owner)),
                    );
                } else if !descend {
                    stack.extend(tree.items.children(item).map(|child| (child, owner)));
                }
            }
        }

        self.scratch.walk = stack;
    }

    fn constraint_viewport_changed(&self) -> bool {
        self.cx.constraint.viewport_changed
    }

    /// Resolve the style of the box `key` of `item` again, and drop the
    /// cached layouts of it and its ancestors. Boxes created by this build
    /// are resolved already.
    fn restyle(&mut self, item: LayoutInputKey, key: BoxFragmentKey) {
        if self.scratch.created.contains_key(key) {
            return;
        }

        let (style, bits) = self.cx.resolve_style(item);
        let in_inline = self
            .fragments
            .boxes
            .parent(key)
            .is_some_and(|parent| self.fragments.boxes[parent].child_layout.is_inline());
        let is_root = item == self.root;
        let fragment = &mut self.fragments.boxes[key];

        if in_inline {
            debug_assert_eq!(
                fragment.box_type == BoxFragmentType::OutOfFlow,
                classify::is_out_of_flow(&style),
                "moving a box inside inline content in or out of flow needs a full layout"
            );
        } else if !is_root {
            fragment.box_type = classify::block_box_type(&style);
        }

        fragment.style = style;
        // Propagated bits of descendants are kept.
        fragment.bits = bits | (fragment.bits & NodeLayoutBits::PROPAGATED);

        self.propagate_bits(key, bits);
        self.invalidate_up(key);
    }
}
