//! A temporary highlight of the text selection: semi transparent rectangles
//! drawn over everything painted, without clipping.

use anyrender::{AntiAliasing, Paint, PaintScene};
use kurbo::{Affine, Rect};
use parley::Cursor;
use peniko::{Color, Fill};

use super::{Window, text_order::LayoutEnd};

/// Semi transparent blue.
const HIGHLIGHT: Color = Color::from_rgba8(0, 120, 255, 96);

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

    /// Fill the highlight of the selected text over `scene`, in physical
    /// pixels.
    pub(super) fn paint_selection(&self, scene: &mut impl PaintScene) {
        let scale = Affine::scale(self.viewport.scale_factor);
        let paint = Paint::Solid(HIGHLIGHT);

        self.for_each_selection_rect(|transform, rect| {
            scene.fill(
                Fill::NonZero,
                scale * transform,
                &paint,
                None,
                &rect,
                AntiAliasing::Enabled,
            );
        });
    }

    /// Call `f` with each rectangle of the highlight, in the coordinates of
    /// its inline layout, and the transform from them to logical scene
    /// coordinates.
    fn for_each_selection_rect(&self, mut f: impl FnMut(Affine, Rect)) {
        let Some(selection) = self.selection.current else {
            return;
        };
        let trees = &self.pipeline.trees;

        for range in self.selected_ranges(selection) {
            let owner = range.owner;
            let (Some(inline), Some(to_scene)) = (
                trees.fragments.get_inline(owner),
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
