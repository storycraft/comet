//! Building an inline formatting context in place, when it is laid out.
//!
//! The text of the inline content is collected and shaped into the
//! [`InlineLayout`](crate::fragment::InlineLayout) the box already owns,
//! keeping its allocations. The boxes inside the inline content are then
//! reconciled with the children of the box, in flow order: boxes of
//! remaining items keep their keys and layouts, and the subtrees of new ones
//! are built right away.

use super::{Engine, taffy::key_of};
use crate::fragment::{BoxFragmentKey, ChildLayoutType};

impl Engine<'_> {
    /// Lay out the box `node` establishing an inline formatting context,
    /// building its inline content first if it is not built. The Taffy
    /// compute function of inline formatting contexts.
    pub(super) fn compute_inline_layout(
        &mut self,
        node: taffy::NodeId,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let key = key_of(node);

        if self
            .fragments
            .inline_layout(key)
            .is_some_and(|inline| !inline.built)
        {
            self.build_inline(key);
        }

        self.layout_inline(key, input)
    }

    /// Build the inline content of the box `key` in place, with the boxes
    /// inside it.
    fn build_inline(&mut self, key: BoxFragmentKey) {
        let fragment = &mut self.fragments.boxes[key];
        let source = fragment.source;
        // Taken out while building, so the boxes can be read.
        let mut inline =
            match std::mem::replace(&mut fragment.child_layout, ChildLayoutType::Content) {
                ChildLayoutType::Inline(inline) => inline,
                other => {
                    fragment.child_layout = other;
                    return;
                }
            };

        let start = inline.run_start;
        self.content
            .build(&self.cx, self.fonts, source, start, &mut inline);
        let content_bits = inline.content_bits;

        let fragment = &mut self.fragments.boxes[key];
        fragment.child_layout = ChildLayoutType::Inline(inline);
        fragment.bits |= content_bits;
        self.propagate_bits(key, content_bits);

        // The boxes inside, whose own child changes were synced by the child
        // sync step.
        let depth = self.scratch.tasks.len();
        let created = self.scratch.created_order.len();
        let children = std::mem::take(&mut self.content.children);
        self.place_children(
            key,
            children
                .iter()
                .map(|&(item, box_type)| (item, Some(box_type))),
            false,
        );
        self.content.children = children;

        if let Some(inline) = self.fragments.boxes[key].inline_mut() {
            inline.atomics.clear();
            inline.atomics.extend(self.scratch.keys.iter().copied());
            inline.built = true;
        }

        // Build the subtrees of the new boxes.
        self.run_tasks(depth);
        self.propagate_created_bits(created);
        self.mark_paint(source);
    }
}
