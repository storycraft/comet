//! A temporary highlight of the text selection: semi transparent rectangles
//! drawn over everything painted, without clipping.

use core::ops::Range;

use anyrender::{AntiAliasing, Paint, PaintScene};
use comet_layout::fragment::FragmentKey;
use kurbo::{Affine, Rect};
use parley::{Affinity, Cursor};
use peniko::{Color, Fill};

use super::Window;
use crate::{
    pipeline::UiTrees,
    selection::{Affinity as TextAffinity, TextPosition},
};

/// Semi transparent blue.
const HIGHLIGHT: Color = Color::from_rgba8(0, 120, 255, 96);

/// An end of the selection in the text of an inline layout: the fragment
/// owning it, the index, and the affinity.
type End = (FragmentKey, usize, TextAffinity);

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
        let Some(selection) = self
            .selection
            .current
            .filter(|selection| !selection.is_collapsed())
        else {
            return;
        };
        let trees = &self.pipeline.trees;
        let Some(root) = trees.box_fragment(self.ui.root()) else {
            return;
        };

        let end = |position: TextPosition| -> Option<End> {
            let (owner, index) = self.layout_position(position)?;
            Some((owner, index, position.affinity))
        };
        let (Some(anchor), Some(extent)) = (end(selection.anchor), end(selection.extent)) else {
            return;
        };

        for (owner, range) in selected_ranges(trees, root, anchor, extent) {
            let (Some(inline), Some(to_scene)) = (
                trees.fragments.get_inline(owner),
                trees.inline_to_scene(owner),
            ) else {
                continue;
            };
            let layout = &inline.layout;
            // Indices past the text, such as of text edited since, end it.
            let cursor = |(index, affinity): (usize, Affinity)| {
                Cursor::from_byte_index(layout, index, affinity)
            };

            parley::Selection::new(cursor(range.start), cursor(range.end))
                .geometry_with(layout, |rect, _| {
                    f(to_scene, Rect::new(rect.x0, rect.y0, rect.x1, rect.y1))
                });
        }
    }
}

/// A part of the text of an inline layout, between the inline boxes
/// holding other inline layouts: positions `start..=end`, with its owner.
struct Segment {
    owner: FragmentKey,
    start: usize,
    end: usize,
}

/// The text of the inline layouts under a fragment, in tree order, split
/// into [`Segment`]s.
///
/// The inline layouts inside an inline box come between the positions
/// before and after its U+FFFC character, so a position is ordered by its
/// segment, then its index. Segments skip those characters.
#[derive(Default)]
struct TextOrder {
    segments: Vec<Segment>,
}

impl TextOrder {
    fn new(trees: &UiTrees, root: FragmentKey) -> Self {
        let mut order = Self::default();
        order.visit(trees, root);
        order
    }

    fn visit(&mut self, trees: &UiTrees, fragment: FragmentKey) {
        let fragments = &trees.fragments;
        let Some(inline) = fragments.get_inline(fragment) else {
            for child in fragments.items.children(fragment) {
                self.visit(trees, child);
            }
            return;
        };

        let mut start = 0;
        for inline_box in inline.layout.inline_boxes() {
            let Some(child) = inline
                .boxes()
                .get(inline_box.id as usize)
                .and_then(|&item| fragments.box_fragment(item))
                .filter(|&child| {
                    fragments
                        .items
                        .descendants(child)
                        .any(|key| fragments.get_inline(key).is_some())
                })
            else {
                continue;
            };

            let index = inline_box.index;
            self.segments.push(Segment {
                owner: fragment,
                start,
                end: index,
            });
            self.visit(trees, child);
            start = index + '\u{FFFC}'.len_utf8();
        }
        self.segments.push(Segment {
            owner: fragment,
            start,
            end: inline.text.len(),
        });

        // Boxes placed under the owner rather than on its lines, such as
        // positioned boxes taken out of the flow.
        for child in fragments.items.children(fragment) {
            if fragments
                .items
                .get(child)
                .is_some_and(|item| item.item.is_box())
            {
                self.visit(trees, child);
            }
        }
    }

    /// Where `end` is in the order: its segment and index. `None` if its
    /// inline layout is not under the root.
    fn key(&self, (owner, index, _): End) -> Option<(usize, usize)> {
        let segment = self.segments.iter().position(|segment| {
            segment.owner == owner && (segment.start..=segment.end).contains(&index)
        })?;

        Some((segment, index))
    }
}

/// The selected range of each inline layout under `root` that `selection`
/// covers, in tree order. The U+FFFC character of an inline box holding
/// other inline layouts is never selected: its text is highlighted on its
/// own, so a selection ending in the box stops before or after it in the
/// text around it. Empty if an end is not laid out.
fn selected_ranges(
    trees: &UiTrees,
    root: FragmentKey,
    anchor: End,
    extent: End,
) -> Vec<(FragmentKey, Range<(usize, Affinity)>)> {
    let order = TextOrder::new(trees, root);
    let (Some(anchor_key), Some(extent_key)) = (order.key(anchor), order.key(extent)) else {
        return Vec::new();
    };
    let ((start, start_affinity), (last, last_affinity)) = {
        let anchor = (anchor_key, Affinity::from(anchor.2));
        let extent = (extent_key, Affinity::from(extent.2));
        if anchor.0 <= extent.0 {
            (anchor, extent)
        } else {
            (extent, anchor)
        }
    };

    (start.0..=last.0)
        .filter_map(|index| {
            let segment = &order.segments[index];
            let from = if index == start.0 {
                (start.1, start_affinity)
            } else {
                (segment.start, Affinity::Downstream)
            };
            let to = if index == last.0 {
                (last.1, last_affinity)
            } else {
                (segment.end, Affinity::Upstream)
            };
            (from.0 < to.0).then_some((segment.owner, from..to))
        })
        .collect()
}
