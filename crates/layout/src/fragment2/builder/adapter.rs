//! What the [Taffy trees](crate::fragment2::taffy) of box fragments need
//! from the fragment tree builder.

use super::Engine;
use crate::fragment2::{BoxFragmentKey, taffy::write_layout};

pub(super) use crate::fragment2::taffy::{BoxTree, key_of, node_id, scrollable_overflow};

impl Engine<'_> {
    /// Write a Taffy layout into the box `key`, marking its source
    /// [`PAINT_CHANGED`](super::PAINT_CHANGED) when it moved, resized or its
    /// overflow changed.
    pub(in crate::fragment2) fn set_unrounded_layout(
        &mut self,
        key: BoxFragmentKey,
        layout: &taffy::Layout,
    ) {
        if write_layout(self.fragments, key, layout) {
            let source = self.fragments.boxes[key].source;
            self.mark_paint(source);
        }
    }
}
