//! Drawing of the fragments of a single paint layer.

use anyrender::{AntiAliasing, PaintRef, PaintScene};
use bytemuck::TransparentWrapper;
use comet_layout::fragment::{
    BoxFragment, BoxFragmentKey, FragmentGeometry, FragmentRef, inline::InlineItem,
    inline::InlineItemKind,
};
use comet_tree::ui::Props;
use kurbo::{Affine, BezPath, Insets, Rect, RoundedRect, RoundedRectRadii, Shape, Stroke, Vec2};
use parley::PositionedLayoutItem;

use super::{Bases, Sources, Transforms, local_rect, push_clip};
use crate::{
    prepaint::{
        FragmentData, JoinedSides, LayoutInputData,
        fragment::{BoxRole, for_each_paint_child_on_lines, paint_props, piece_of_part},
    },
    style::{self, Background, BorderFill, BoxDecorationBreak, OutlineFill, Paint},
};

/// Draws the fragments painting in a single paint layer into a scene: its
/// owner's background, border and outline, and its owner's descendants in
/// paint order, skipping the fragments starting child layers and their
/// descendants. Boxes inside the layer clip their own descendants.
///
/// The layer itself, its transform, scroll offset, filters, opacity and the
/// clips around it, are applied by the
/// [`CometCompositePainter`](super::CometCompositePainter) painting it, so a
/// layer can also be drawn on its own, such as into a compositor surface.
/// Keep it to reuse its buffers across paints.
#[derive(Default)]
pub struct CometLayerPainter {
    /// Fragments to paint in the layer being painted.
    pending: Vec<Visit>,
    /// Selection area behind the glyph run being painted.
    selection: Vec<Rect>,
}

/// A step of painting the fragments of a layer.
enum Visit {
    /// Paint a fragment and queue its children.
    Fragment {
        fragment: FragmentRef,
        /// Maps the coordinates of the contents of the box the fragment is
        /// placed in to scene coordinates: its parent box, or the owner of
        /// the inline layout holding an inline item.
        parent: Transforms,
        /// Scene bounds of the clips the fragment paints inside of.
        clip: Option<Rect>,
    },
    /// Pop the clip layer of a fragment's contents.
    PopClip,
}

impl CometLayerPainter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop the fragments left to paint.
    pub(super) fn clear(&mut self) {
        self.pending.clear();
    }

    /// Paint the background, border and outline of `owner`, the owner of a
    /// layer, with `transforms` mapping it to the scene: the inline part box
    /// of every piece of an inline element, placed from `bases`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_owner(
        &mut self,
        sources: Sources<'_>,
        owner: BoxFragmentKey,
        fragment: &BoxFragment,
        transforms: Transforms,
        bases: &Bases,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) {
        if BoxRole::of(fragment) != BoxRole::InlinePart {
            draw_box_fragment(sources, owner, fragment, transforms, clip, scene);
            return;
        }

        for part in sources.tree.split_boxes(owner) {
            if let Some(((part_transforms, _), part_fragment)) = sources
                .part_transforms(bases, part)
                .zip(sources.tree.get(part))
            {
                draw_box_fragment(sources, part, part_fragment, part_transforms, clip, scene);
            }
        }
    }

    /// Paint the descendants of `owner`, the owner of a layer, painting in
    /// the layer, with `transforms` mapping it to the scene: the items of
    /// every piece of an inline element. Its contents are placed from
    /// `bases`, so they move with its scroll offset, and paint inside `clip`.
    /// Skipped when they are out of view.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_contents(
        &mut self,
        sources: Sources<'_>,
        owner: BoxFragmentKey,
        fragment: &BoxFragment,
        transforms: Transforms,
        bases: &Bases,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) {
        if BoxRole::of(fragment) != BoxRole::InlinePart {
            if sources.visible(
                transforms.transform,
                fragment.layout.geometry.overflow,
                clip,
            ) && let Some(contents) = bases.get(owner)
            {
                self.draw_descendants(sources, FragmentRef::Box(owner), contents, clip, scene);
            }
            return;
        }

        // Each piece paints its items in turn.
        for part in sources.tree.split_boxes(owner) {
            let (Some(piece), Some((part_transforms, parent)), Some(part_fragment)) = (
                piece_of_part(sources.tree, part),
                sources.part_transforms(bases, part),
                sources.tree.get(part),
            ) else {
                continue;
            };

            if sources.visible(
                part_transforms.transform,
                part_fragment.layout.geometry.overflow,
                clip,
            ) {
                self.draw_descendants(sources, piece, parent, clip, scene);
            }
        }
    }

    /// Paint the descendants of `fragment` in paint order, skipping the
    /// fragments starting child layers and their descendants. `parent` maps
    /// the coordinates the children of `fragment` are placed in to the scene:
    /// the scrolled contents of a box, or those of the owner of the inline
    /// layout holding an inline item.
    fn draw_descendants(
        &mut self,
        sources: Sources<'_>,
        fragment: FragmentRef,
        parent: Transforms,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) {
        let base = self.pending.len();
        self.push_children(sources, fragment, parent, clip);

        while self.pending.len() > base {
            let Some(visit) = self.pending.pop() else {
                break;
            };

            let (fragment, parent, clip) = match visit {
                Visit::Fragment {
                    fragment,
                    parent,
                    clip,
                } => (fragment, parent, clip),
                Visit::PopClip => {
                    scene.pop_layer();
                    continue;
                }
            };

            // Child layers paint on their own, and the layer of an inline
            // element with every piece of it and their items.
            if sources.starts_layer(fragment) {
                continue;
            }

            match fragment {
                FragmentRef::Box(key) => {
                    let (Some(box_fragment), Some(own)) = (sources.tree.get(key), sources.own(key))
                    else {
                        continue;
                    };

                    let transforms = parent.then(own);
                    draw_box_fragment(sources, key, box_fragment, transforms, clip, scene);

                    // Nothing below paints in view: skip the whole subtree.
                    if !sources.visible(
                        transforms.transform,
                        box_fragment.layout.geometry.overflow,
                        clip,
                    ) {
                        continue;
                    }

                    // A clipping box clips its descendants, not itself.
                    let (child_clip, clipped) =
                        push_clip(sources, key, box_fragment, transforms, clip, scene);

                    if clipped {
                        self.pending.push(Visit::PopClip);
                    }

                    let contents = transforms.contents(sources.layers, key);
                    self.push_children(sources, fragment, contents, child_clip);
                }
                FragmentRef::Inline { owner, index } => {
                    let Some(inline) = sources.tree.inline_layout(owner) else {
                        continue;
                    };
                    let Some(item) = inline.items.get(index as usize) else {
                        continue;
                    };

                    match item.kind {
                        InlineItemKind::Run { line, run } => {
                            self.draw_glyph_run(
                                sources,
                                owner,
                                item,
                                (line as usize, run as usize),
                                parent,
                                clip,
                                scene,
                            );
                        }
                        InlineItemKind::Box { child } => {
                            // A piece of an inline element paints its inline
                            // part box under its items.
                            if let Some(part) = child
                                && let (Some(part_fragment), Some(own)) =
                                    (sources.tree.get(part), sources.own(part))
                            {
                                let transforms = parent.then(own);
                                draw_box_fragment(
                                    sources,
                                    part,
                                    part_fragment,
                                    transforms,
                                    clip,
                                    scene,
                                );

                                if !sources.visible(
                                    transforms.transform,
                                    item.geometry.overflow,
                                    clip,
                                ) {
                                    continue;
                                }
                            }

                            // Its items are placed in the same box.
                            self.push_children(sources, fragment, parent, clip);
                        }
                        InlineItemKind::Line { .. } => {
                            self.push_children(sources, fragment, parent, clip);
                        }
                    }
                }
            }
        }
    }

    /// Queue the children of `fragment` in paint order, placed with
    /// `parent`, skipping the items of lines out of view.
    fn push_children(
        &mut self,
        sources: Sources<'_>,
        fragment: FragmentRef,
        parent: Transforms,
        clip: Option<Rect>,
    ) {
        let start = self.pending.len();

        for_each_paint_child_on_lines(
            sources.tree,
            fragment,
            |line| sources.visible(parent.transform, line.geometry.extent(), clip),
            |child| {
                self.pending.push(Visit::Fragment {
                    fragment: child,
                    parent,
                    clip,
                });
            },
        );

        // Popped from the last.
        self.pending[start..].reverse();
    }

    /// Paint the glyph run `item` of the inline layout of `owner`, the run
    /// `run` of its line `line`, in the coordinates of the contents of `owner`,
    /// which `parent` maps to the scene. `None` if nothing was drawn: the run
    /// is missing, out of view, or its item has no resolved text color.
    #[allow(clippy::too_many_arguments)]
    fn draw_glyph_run(
        &mut self,
        sources: Sources<'_>,
        owner: BoxFragmentKey,
        item: &InlineItem,
        (line, run): (usize, usize),
        parent: Transforms,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) -> Option<()> {
        let inline = sources.tree.inline_layout(owner)?;
        let PositionedLayoutItem::GlyphRun(glyph_run) =
            inline.layout.get(line)?.items().nth(run)?
        else {
            return None;
        };

        let font_run = glyph_run.run();
        let geometry = &item.geometry;
        let rect = local_rect(geometry) + Vec2::new(geometry.location.x, geometry.location.y);

        // Glyph ink can extend past the run's line box, such as accents above a
        // tight line height or italic overhangs. One em on each side bounds it.
        let overhang = font_run.font_size() as f64;

        if !sources.visible(parent.transform, rect.inflate(overhang, overhang), clip) {
            return None;
        }

        // The brush of the run's style is the text item it was shaped from.
        // Runs without one belong to the owner.
        let key = glyph_run
            .style()
            .brush
            .or_else(|| sources.tree.get(owner).map(|owner| owner.source))?;

        // Resolved by the pre-paint step. Runs of items it missed are skipped.
        let data = sources.prepaint.get(key)?;
        let anti_aliasing = paint_props(sources.ui, sources.inputs, key)
            .map_or(AntiAliasing::Enabled, anti_aliasing);

        // Parley's coordinates wrap the raw `i16` bits Anyrender takes.
        let normalized_coords: &[i16] =
            TransparentWrapper::peel_slice(font_run.normalized_coords());

        // Parley's glyphs are positioned within the inline layout, whose origin
        // sits at the owner's content box.
        let origin = inline.origin();
        let (offset_x, offset_y) = (origin.x as f32, origin.y as f32);

        // The selection area of the item behind this run: its rectangles
        // across the run, from the top to the bottom of their lines.
        self.selection.clear();
        self.selection.extend(
            sources
                .selection
                .rects(key)
                .iter()
                .map(|area| *area + origin)
                .filter(|area| area.y0 < rect.y1 && area.y1 > rect.y0)
                .map(|area| Rect::new(area.x0.max(rect.x0), area.y0, area.x1.min(rect.x1), area.y1))
                .filter(|area| area.x0 < area.x1),
        );

        if let Some(background) = scene_paint(&data.selection_background) {
            for area in &self.selection {
                scene.fill(
                    peniko::Fill::NonZero,
                    parent.transform,
                    background.clone(),
                    None,
                    area,
                    anti_aliasing,
                );
            }
        }

        // Glyphs are shaped at their logical size but drawn at their physical
        // font size, so they are hinted for the display's pixels. Their positions
        // scale with them, and the display scale is taken out of the transform.
        let scale_factor = sources.scale_factor;
        let scale = scale_factor as f32;
        let draw = |scene: &mut _, paint: &Paint| {
            let Some(paint) = scene_paint(paint) else {
                return;
            };

            draw_glyphs(
                scene,
                font_run.font(),
                font_run.font_size() * scale,
                normalized_coords,
                paint,
                parent.transform * Affine::scale(scale_factor.recip()),
                glyph_run.positioned_glyphs().map(|glyph| anyrender::Glyph {
                    id: glyph.id,
                    x: (offset_x + glyph.x) * scale,
                    y: (offset_y + glyph.y) * scale,
                }),
                anti_aliasing,
            )
        };

        draw(scene, &data.color);

        // Selected glyphs are drawn again over themselves, in the selection
        // color, clipped to the selection area.
        if !self.selection.is_empty() {
            let mut shape = BezPath::new();
            for area in &self.selection {
                shape.extend(area.path_elements(0.1));
            }

            scene.push_clip_layer(peniko::Fill::NonZero, parent.transform, &shape);
            draw(scene, &data.selection_color);
            scene.pop_layer();
        }

        Some(())
    }
}

/// Draw `glyphs` of `font` filled with `paint`, with `transform`.
#[allow(clippy::too_many_arguments)]
fn draw_glyphs(
    scene: &mut impl PaintScene,
    font: &peniko::FontData,
    font_size: f32,
    normalized_coords: &[i16],
    paint: PaintRef<'_>,
    transform: Affine,
    glyphs: impl Iterator<Item = anyrender::Glyph> + Clone,
    anti_aliasing: AntiAliasing,
) {
    scene.draw_glyphs(
        font,
        font_size,
        true,
        normalized_coords,
        Vec2::ZERO,
        peniko::StyleRef::Fill(peniko::Fill::NonZero),
        paint,
        1.0,
        transform,
        None,
        glyphs,
        anti_aliasing,
    );
}

/// Paint the background, border and outline of a box, without its
/// descendants, inside the clips bounded by `clip`. Anonymous boxes paint
/// nothing, and neither do the boxes of text items, which have no paint
/// data.
fn draw_box_fragment(
    sources: Sources<'_>,
    key: BoxFragmentKey,
    fragment: &BoxFragment,
    transforms: Transforms,
    clip: Option<Rect>,
    scene: &mut impl PaintScene,
) {
    if !BoxRole::of(fragment).decorates() {
        return;
    }

    let data = sources.prepaint.get(fragment.source);
    let geometry = &fragment.layout.geometry;
    let rect = local_rect(geometry);

    if sources.visible(transforms.transform, box_bounds(geometry, data, rect), clip) {
        draw_box(
            sources,
            fragment,
            data,
            sources.prepaint.fragment(key),
            rect,
            transforms.transform,
            scene,
        );
    }
}

/// Bounds of the background, border and outline painted for a box's border
/// box `rect`. The border is stroked outside the box, up to its widest side,
/// and the outline past its offset.
fn box_bounds(geometry: &FragmentGeometry, data: Option<&LayoutInputData>, rect: Rect) -> Rect {
    let outline = data
        .filter(|data| data.outline_width > 0.0)
        .map_or(0.0, |data| data.outline_offset + data.outline_width);
    let width = border_width(geometry).max(outline);

    rect + Insets::uniform(width)
}

/// The area a border `width` wide stroked outside the border box `rect`
/// paints in, cut at the sides in `joins`.
fn border_clip(rect: Rect, width: f64, joins: JoinedSides) -> Rect {
    let side = |joined: bool| if joined { 0.0 } else { width };

    rect + Insets::new(
        side(joins.left),
        side(joins.top),
        side(joins.right),
        side(joins.bottom),
    )
}

fn border_width(geometry: &FragmentGeometry) -> f64 {
    let border = geometry.border;

    border
        .left_top
        .x
        .max(border.left_top.y)
        .max(border.right_bottom.x)
        .max(border.right_bottom.y)
}

/// Paint the background, border and outline of a box's border box `rect`.
/// The background and border are rounded with the box's
/// [border radii](FragmentData::border_radii) from `paint_data`, square at
/// the joins of a split box unless it clones its decoration, and the outline with all four corners of its
/// [border radius](FragmentData::border_radius).
fn draw_box(
    sources: Sources<'_>,
    fragment: &BoxFragment,
    data: Option<&LayoutInputData>,
    paint_data: Option<&FragmentData>,
    rect: Rect,
    transform: Affine,
    scene: &mut impl PaintScene,
) {
    let Some(props) = paint_props(sources.ui, sources.inputs, fragment.source) else {
        return;
    };
    let anti_aliasing = anti_aliasing(props);
    let decoration_break = data.map_or_else(BoxDecorationBreak::default, |data| {
        data.box_decoration_break
    });
    let radii = paint_data.map_or(RoundedRectRadii::from(0.0), |data| {
        data.border_radii(fragment, decoration_break)
    });

    if let Some(fill) = props.get::<Background>()
        && let Some(paint) = scene_paint(&fill.0)
    {
        scene.fill(
            peniko::Fill::NonZero,
            transform,
            paint,
            None,
            &rect.to_rounded_rect(radii),
            anti_aliasing,
        );
    }

    let width = border_width(&fragment.layout.geometry);

    if width > 0.0
        && let Some(fill) = props.get::<BorderFill>()
        && let Some(paint) = scene_paint(&fill.0)
    {
        // A broken box leaves out the border on the sides joining its other
        // fragments: the border is stroked outside the box, so clipping at
        // those sides cuts it there, and the sides around run on into the
        // next fragment.
        let joins = match decoration_break {
            BoxDecorationBreak::Break => JoinedSides::of(fragment),
            BoxDecorationBreak::Clone => JoinedSides::default(),
        };
        let clipped = !joins.is_empty();
        if clipped {
            scene.push_clip_layer(
                peniko::Fill::NonZero,
                transform,
                &border_clip(rect, width, joins),
            );
        }

        scene.stroke(
            &Stroke::new(width),
            transform,
            paint,
            None,
            &rect.inset(-width / 2.0).to_rounded_rect(radii),
            anti_aliasing,
        );

        if clipped {
            scene.pop_layer();
        }
    }

    if let Some(data) = data.filter(|data| data.outline_width > 0.0)
        && let Some(fill) = props.get::<OutlineFill>()
        && let Some(paint) = scene_paint(&fill.0)
    {
        let width = data.outline_width;

        scene.stroke(
            &Stroke::new(width),
            transform,
            paint,
            None,
            &outline_path(
                rect,
                paint_data
                    .and_then(|data| data.border_radius)
                    .unwrap_or_default(),
                data.outline_offset,
                width,
            ),
            anti_aliasing,
        );
    }
}

/// The scene paint of `paint`. `None` for
/// [content](Paint::Content) paints, which are not drawn yet.
fn scene_paint(paint: &Paint) -> Option<PaintRef<'_>> {
    match paint {
        Paint::Solid(color) => Some(anyrender::Paint::Solid(*color)),
        Paint::Gradient(gradient) => Some(anyrender::Paint::Gradient(gradient)),
        // TODO: draw the content over the filled shape.
        Paint::Content(_) => None,
    }
}

/// How a node draws the edges of its shapes and glyphs, from its
/// [`AntiAliasing`](style::AntiAliasing) in `props`.
fn anti_aliasing(props: Props<'_>) -> AntiAliasing {
    if props.get_or_default::<style::AntiAliasing>().0 {
        AntiAliasing::Enabled
    } else {
        AntiAliasing::Disabled
    }
}

/// The path an outline of `width` is stroked along: each edge of the border
/// box `rect` moved out by `offset` and half the width, rounded with all four
/// border `radii` grown by the same distance. Square corners stay square, and a
/// negative offset shrinks the box no further than its centre.
fn outline_path(rect: Rect, radii: RoundedRectRadii, offset: f64, width: f64) -> RoundedRect {
    let shrink = rect.width().min(rect.height()) / 2.0;
    let distance = (offset + width / 2.0).max(-shrink);
    let grow = |radius: f64| {
        if radius > 0.0 {
            (radius + distance).max(0.0)
        } else {
            0.0
        }
    };
    (rect + Insets::uniform(distance)).to_rounded_rect(RoundedRectRadii::new(
        grow(radii.top_left),
        grow(radii.top_right),
        grow(radii.bottom_right),
        grow(radii.bottom_left),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn border_clips_cut_the_joined_sides() {
        let rect = Rect::new(10.0, 20.0, 50.0, 40.0);

        // Closed boxes clip nowhere inside their border.
        assert_eq!(
            border_clip(rect, 2.0, JoinedSides::default()),
            Rect::new(8.0, 18.0, 52.0, 42.0)
        );

        // The middle piece of an inline element is open on both ends.
        let middle = JoinedSides {
            left: true,
            right: true,
            ..JoinedSides::default()
        };
        assert_eq!(
            border_clip(rect, 2.0, middle),
            Rect::new(10.0, 18.0, 50.0, 42.0)
        );

        // The first fragment of a box split down the page is open below.
        let first = JoinedSides {
            bottom: true,
            ..JoinedSides::default()
        };
        assert_eq!(
            border_clip(rect, 2.0, first),
            Rect::new(8.0, 18.0, 52.0, 40.0)
        );
    }
}
