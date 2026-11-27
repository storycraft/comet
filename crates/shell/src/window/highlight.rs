//! Geometry of the text selection, which the painter fills behind the
//! selected glyphs.

use comet_paint::selection::order::{LayoutEnd, selected_ranges};
use kurbo::{Affine, Rect};
use parley::Cursor;

use super::Window;

impl Window {
    /// The bounding boxes of the highlight of the selected text, from the
    /// last layout, in logical scene coordinates. Empty without a selection
    /// or when it is collapsed.
    pub fn selection_rects(&self) -> Vec<Rect> {
        let mut rects = Vec::new();
        self.for_each_selection_rect(|transform, rect| {
            rects.push(transform.transform_rect_bbox(rect));
        });

        rects
    }

    /// Call `f` with each rectangle of the highlight, in the coordinates of
    /// its inline layout, and the transform from them to logical scene
    /// coordinates.
    fn for_each_selection_rect(&self, mut f: impl FnMut(Affine, Rect)) {
        let Some(selection) = self.selection.current else {
            return;
        };
        let trees = &self.pipeline.trees;
        let Some(root) = trees.box_fragment(self.ui.root()) else {
            return;
        };
        let tree = self.ui.tree().borrow();

        for range in selected_ranges(&tree.ui, &trees.inputs, &trees.fragments, root, selection) {
            let owner = range.owner;
            let (Some(inline), Some(to_scene)) = (
                trees.fragments.inline_layout(owner),
                trees.inline_to_scene(owner),
            ) else {
                continue;
            };
            let layout = &inline.layout;
            // Indices past the text, such as of text edited since, end it.
            let cursor =
                |(index, affinity): LayoutEnd| Cursor::from_byte_index(layout, index, affinity);

            parley::Selection::new(cursor(range.start), cursor(range.end))
                .geometry_with(layout, |rect, _| {
                    f(to_scene, Rect::new(rect.x0, rect.y0, rect.x1, rect.y1))
                });
        }
    }
}
