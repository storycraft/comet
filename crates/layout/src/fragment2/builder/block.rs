//! Step 3 of a build: lay out the boxes.
//!
//! Block boxes use Taffy's block algorithm through the [adapter](super::adapter),
//! inline formatting contexts are [built](super::content) and laid out by
//! their [lines](super::lines), and other boxes are leaves sized by their
//! style.

use taffy::style::CoreStyle;

use super::{
    Engine, PAINT_CHANGED,
    adapter::{BoxTree, node_id},
    classify::ChildKind,
};
use crate::{
    fragment2::{BoxFragmentKey, BoxFragmentType},
    style::geometry::Point,
    taffy::conv,
};

impl Engine<'_> {
    /// Lay out the box of the build root in the available space of the
    /// build constraint.
    pub(super) fn layout(&mut self, root: BoxFragmentKey) {
        let available = conv::to_size(self.cx.constraint.available_size, |v| {
            taffy::AvailableSpace::Definite(v as f32)
        });

        self.compute(root, available);
    }

    /// Lay out a box as an independent root of the available space, and
    /// record its baseline.
    pub(super) fn compute(
        &mut self,
        key: BoxFragmentKey,
        available: taffy::Size<taffy::AvailableSpace>,
    ) {
        if self.fragments.get(key).is_none() {
            return;
        }

        taffy::compute_root_layout(&mut BoxTree::root_pass(self, key), node_id(key), available);
        // The root positioning pass places the remaining out-of-flow boxes
        // again, even when the root layout is cached.
        self.place_hoisted(key);

        let baseline = self.inline_baseline(key);
        self.fragments.boxes[key].layout.baseline = baseline;
    }

    /// Lay out a box with the algorithm of its content.
    pub(in crate::fragment2) fn layout_box(
        &mut self,
        key: BoxFragmentKey,
        input: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        let Some(fragment) = self.fragments.get(key) else {
            return taffy::LayoutOutput::HIDDEN;
        };

        match ChildKind::of(&fragment.child_layout) {
            ChildKind::Block => self.layout_block(key, input, block_ctx),
            ChildKind::Inline => self.compute_inline_layout(node_id(key), input),
            // TODO(container): custom container algorithms need their trait
            // integrated with box fragments. Until then containers are
            // leaves, and their children are not laid out.
            ChildKind::Container | ChildKind::Content => self.layout_leaf(key, input),
        }
    }

    /// Lay out a block container with Taffy's block algorithm, then position
    /// the out-of-flow boxes it contains.
    fn layout_block(
        &mut self,
        key: BoxFragmentKey,
        input: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        let id = node_id(key);
        let mut engine = BoxTree::new(self, key);
        let mut output = taffy::compute_block_layout(&mut engine, id, input, block_ctx);

        if input.run_mode == taffy::RunMode::PerformLayout {
            taffy::compute_oof_layout(&mut engine, id, &mut output);
            drop(engine);
            self.place_hoisted(key);
        }

        output
    }

    /// Lay out a box without children, sized by its style alone.
    fn layout_leaf(
        &mut self,
        key: BoxFragmentKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let style = &self.fragments.boxes[key].style;

        taffy::compute_leaf_layout(input, style, |_, _| 0.0, |_, _| taffy::Size::ZERO)
    }

    /// Make the out-of-flow boxes positioned by `containing` relative to
    /// their parent boxes again.
    ///
    /// Taffy places them relative to their containing block, which can be an
    /// ancestor of their parent. Their retained Taffy layouts keep containing
    /// block coordinates, so this can run again after every positioning pass.
    pub(super) fn place_hoisted(&mut self, containing: BoxFragmentKey) {
        let mut hoisted = std::mem::take(&mut self.scratch.hoisted);
        hoisted.clear();
        hoisted.extend_from_slice(&self.fragments.boxes[containing].hoisted);

        let tree = self.cx.tree;
        let boxes = &mut self.fragments.boxes;

        for &key in &hoisted {
            if boxes.get(key).is_none() {
                continue;
            }

            let (mut x, mut y) = (0.0, 0.0);
            let mut ancestor = boxes.parent(key);

            while let Some(parent) = ancestor
                && parent != containing
            {
                let location = boxes[parent].layout.geometry.location;
                x += location.x;
                y += location.y;
                ancestor = boxes.parent(parent);
            }

            // Not attached below its containing block.
            if ancestor.is_none() {
                continue;
            }

            let location = boxes[key].layout.unrounded.location;
            let location = Point::new(f64::from(location.x) - x, f64::from(location.y) - y);

            if boxes[key].layout.geometry.location != location {
                boxes[key].layout.geometry.location = location;
                self.dirty.mark(tree, boxes[key].source, PAINT_CHANGED);
            }

            // Taffy adds the box to the overflow of its containing block
            // alone, so the boxes in between cover it too.
            let mut extent = boxes[key].layout.geometry.extent();
            let mut ancestor = boxes.parent(key);

            while let Some(parent) = ancestor
                && parent != containing
            {
                let source = boxes[parent].source;
                let geometry = &mut boxes[parent].layout.geometry;
                let grown = geometry.overflow.union(extent);

                if grown != geometry.overflow {
                    geometry.overflow = grown;
                    self.dirty.mark(tree, source, PAINT_CHANGED);
                }

                extent = geometry.extent();
                ancestor = boxes.parent(parent);
            }
        }

        self.scratch.hoisted = hoisted;
    }

    /// Mark the final layout of a box and its ancestors invalid. Measuring
    /// an inline layout replaces its lines, so the final layout must run
    /// again even when its cache entry matches.
    pub(super) fn invalidate_final(&mut self, key: BoxFragmentKey) {
        let mut current = Some(key);

        while let Some(key) = current {
            let Some(fragment) = self.fragments.get_mut(key) else {
                return;
            };
            fragment.final_layout_valid = false;

            if Some(key) == self.root_box {
                return;
            }

            current = self.fragments.boxes.parent(key);
        }
    }

    /// Last in-flow line baseline, from the border box top.
    fn last_baseline(&self, key: BoxFragmentKey) -> Option<f32> {
        let fragment = self.fragments.get(key)?;
        let layout = &fragment.layout.unrounded;

        if let Some(inline) = fragment.inline() {
            return inline
                .last_baseline
                .map(|baseline| layout.border.top + layout.padding.top + baseline);
        }

        if !fragment.child_layout.is_block() {
            return None;
        }

        self.fragments.boxes.children(key).rev().find_map(|child| {
            let fragment = &self.fragments.boxes[child];

            if fragment.box_type == BoxFragmentType::OutOfFlow {
                return None;
            }

            self.last_baseline(child)
                .map(|baseline| fragment.layout.unrounded.location.y + baseline)
        })
    }

    /// Baseline aligning a box on a line, from the border box top. Without
    /// a line inside, or when clipping its overflow, the bottom margin edge.
    pub(super) fn inline_baseline(&self, key: BoxFragmentKey) -> Option<f32> {
        let fragment = self.fragments.get(key)?;
        let overflow = fragment.style.overflow();

        if overflow.x == taffy::Overflow::Visible
            && overflow.y == taffy::Overflow::Visible
            && let Some(baseline) = self.last_baseline(key)
        {
            return Some(baseline);
        }

        let layout = &fragment.layout.unrounded;
        Some(layout.size.height + layout.margin.bottom)
    }
}
