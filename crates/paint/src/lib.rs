pub mod pre_paint;
pub mod prelude;
pub mod style;

use anyrender::PaintScene;
use bytemuck::TransparentWrapper;
use comet_layout::{
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKind, tree::LayoutInputTree},
};
use comet_tree::{node::NodeKey, ui::Ui};
use kurbo::{Affine, Point, Rect, Stroke, Vec2};
use parley::PositionedLayoutItem;

use crate::{
    pre_paint::{FragmentData, LayoutInputData, tree::PrePaintTree},
    style::{BorderFill, Fill},
};

/// Paints final fragments in tree order, with the [`PrePaintTree`] resolved
/// for them.
#[derive(Default)]
pub struct CometPainter {
    pending: Vec<Visit>,
    /// Clips of the pushed clip layers, each intersected with the ones below
    /// it, to skip fragments painting outside them.
    clips: Vec<Rect>,
}

enum Visit {
    /// Paint a fragment, inside the line containing it.
    Fragment {
        key: FragmentKey,
        inline_owner: Option<FragmentKey>,
        /// Maps the local coordinates of the fragment's parent to scene
        /// coordinates.
        parent: Transforms,
    },
    /// Pop the clip layer of a fragment's contents.
    PopClip,
}

/// A transform from local fragment coordinates to scene coordinates, and its
/// inverse.
#[derive(Clone, Copy)]
struct Transforms {
    transform: Affine,
    /// `None` when the transform collapses an axis and has no inverse.
    inverse: Option<Affine>,
}

impl Transforms {
    const IDENTITY: Self = Self {
        transform: Affine::IDENTITY,
        inverse: Some(Affine::IDENTITY),
    };

    /// The transforms of a fragment whose parent maps to the scene with
    /// `self`.
    fn then(self, own: &FragmentData) -> Self {
        Self {
            transform: self.transform * own.transform,
            inverse: own
                .inverse_transform
                .zip(self.inverse)
                .map(|(own, parent)| own * parent),
        }
    }

    /// The transforms mapping the local coordinates of `key` to the scene,
    /// multiplied from the root down.
    fn of(tree: &FragmentTree, pre_paint: &PrePaintTree, key: FragmentKey) -> Self {
        let parent = match tree.items.parent(key) {
            Some(parent) => Self::of(tree, pre_paint, parent),
            None => Self::IDENTITY,
        };

        parent.then(&pre_paint[key])
    }
}

impl CometPainter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Paint the fragments under `root`. `inputs` is the input tree the
    /// fragments were built from, and `pre_paint` must be resolved for them.
    /// Each fragment paints with its resolved data, such as its transform and
    /// border radius, and the text color of its source item. Fragment
    /// transforms multiply down the tree, starting with the transforms of
    /// `root`'s ancestors.
    ///
    /// A box fragment clipping its overflow pushes a clip layer of its border
    /// box for its contents, rounded with its border radius. Clips of `root`'s ancestors do not apply.
    ///
    /// Fragments painting entirely outside `viewport` or their clip, in scene
    /// coordinates, are skipped. Fragments touching their edges are painted.
    /// Their descendants are still visited, since children may paint outside
    /// their parents.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        tree: &FragmentTree,
        pre_paint: &PrePaintTree,
        root: FragmentKey,
        viewport: Rect,
        scene: &mut impl PaintScene,
    ) {
        self.pending.clear();
        self.clips.clear();
        self.pending.push(Visit::Fragment {
            key: root,
            inline_owner: None,
            parent: match tree.items.parent(root) {
                Some(parent) => Transforms::of(tree, pre_paint, parent),
                None => Transforms::IDENTITY,
            },
        });

        while let Some(visit) = self.pending.pop() {
            let (key, mut inline_owner, parent) = match visit {
                Visit::Fragment {
                    key,
                    inline_owner,
                    parent,
                } => (key, inline_owner, parent),
                Visit::PopClip => {
                    scene.pop_layer();
                    self.clips.pop();
                    continue;
                }
            };
            let fragment = &tree.items[key];

            if matches!(fragment.item, FragmentItem::Line { .. }) {
                inline_owner = Some(key);
            }

            let own = &pre_paint[key];
            let transforms = parent.then(own);
            let data = &pre_paint[fragment.source];
            let clip = self.clips.last().copied();
            self.draw_fragment(
                ui,
                inputs,
                tree,
                key,
                inline_owner,
                own,
                transforms.transform,
                data,
                viewport,
                clip,
                scene,
            );

            // A clipping fragment clips the contents of its item.
            if let Some(layer) = clip_layer(fragment, own, transforms, viewport) {
                let transform = transforms.transform;

                // Clipping on both axes follows the rounded corners of the box.
                // TODO:: horizontal, vertical curve clip for round rect.
                match own.border_radius.filter(|_| own.clip == (true, true)) {
                    Some(radii) => scene.push_clip_layer(
                        peniko::Fill::NonZero,
                        transform,
                        &layer.to_rounded_rect(radii),
                    ),
                    None => scene.push_clip_layer(peniko::Fill::NonZero, transform, &layer),
                }

                let bounds = transform.transform_rect_bbox(layer);
                self.clips
                    .push(clip.map_or(bounds, |clip| clip.intersect(bounds)));
                self.pending.push(Visit::PopClip);
            }

            self.pending
                .extend(tree.items.children(key).rev().map(|child| Visit::Fragment {
                    key: child,
                    inline_owner,
                    parent: transforms,
                }));
        }
    }

    /// Paint one fragment itself, inside `clip`, in its local coordinates
    /// mapped to the scene by `transform`.
    #[allow(clippy::too_many_arguments)]
    fn draw_fragment(
        &self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        tree: &FragmentTree,
        key: FragmentKey,
        inline_owner: Option<FragmentKey>,
        own: &FragmentData,
        transform: Affine,
        data: &LayoutInputData,
        viewport: Rect,
        clip: Option<Rect>,
        scene: &mut impl PaintScene,
    ) {
        let fragment = &tree.items[key];
        let rect = local_rect(fragment);
        let visible = |bounds: Rect| {
            let bounds = transform.transform_rect_bbox(bounds);

            bounds.overlaps(viewport) && clip.is_none_or(|clip| bounds.overlaps(clip))
        };

        match fragment.item {
            FragmentItem::Box { .. } => {
                // A standalone text root also owns a layout box. Its paint
                // belongs to its glyph fragments, not to both representations.
                if inputs.items[fragment.source].kind != LayoutInputKind::Text
                    && visible(box_bounds(fragment, rect))
                {
                    draw_box(ui, inputs, fragment, own, rect, transform, scene);
                }
            }
            FragmentItem::Inline { .. } => {
                if visible(box_bounds(fragment, rect)) {
                    draw_box(ui, inputs, fragment, own, rect, transform, scene);
                }
            }
            // Anonymous blocks only position their lines.
            FragmentItem::AnonymousBlock { .. } => {}
            FragmentItem::Line { .. } => {}
            FragmentItem::GlyphRun { line, item } => {
                if visible(box_bounds(fragment, rect)) {
                    draw_box(ui, inputs, fragment, own, rect, transform, scene);
                }

                if let Some(owner) = inline_owner {
                    draw_glyph_run(
                        tree,
                        fragment,
                        owner,
                        (line, item),
                        rect,
                        transform,
                        &data.text_color,
                        &visible,
                        scene,
                    );
                }
            }
        }
    }
}

/// The border box of a fragment, in its local coordinates.
fn local_rect(fragment: &Fragment) -> Rect {
    Rect::from_origin_size(
        Point::ORIGIN,
        (fragment.layout.size.width, fragment.layout.size.height),
    )
}

/// The clip of a fragment clipping its contents, in its local coordinates:
/// its border box on the clipped axes. Unclipped axes are bounded by the
/// viewport, mapped back to local coordinates with `transforms`, the
/// fragment's transforms to the scene. Fragments whose scene transform has no
/// inverse push no clip layer.
///
/// Fragments clipping on both axes round the layer with their border radius.
/// Clipping on one axis keeps it square, as every corner then lies on the
/// viewport bound of the unclipped axis.
fn clip_layer(
    fragment: &Fragment,
    own: &FragmentData,
    transforms: Transforms,
    viewport: Rect,
) -> Option<Rect> {
    let (x, y) = own.clip;

    if !x && !y {
        return None;
    }

    // A collapsed box paints its contents collapsed with it.
    let visible = transforms.inverse?.transform_rect_bbox(viewport);
    let bounds = local_rect(fragment);

    Some(Rect::new(
        if x { bounds.x0 } else { visible.x0 },
        if y { bounds.y0 } else { visible.y0 },
        if x { bounds.x1 } else { visible.x1 },
        if y { bounds.y1 } else { visible.y1 },
    ))
}

/// Bounds of the background and border painted for a fragment's border box.
/// The border is stroked outside the box, up to its widest side.
fn box_bounds(fragment: &Fragment, rect: Rect) -> Rect {
    let width = border_width(fragment);

    rect.inflate(width, width)
}

fn border_width(fragment: &Fragment) -> f64 {
    let border = fragment.layout.border;

    border
        .left_top
        .x
        .max(border.left_top.y)
        .max(border.right_bottom.x)
        .max(border.right_bottom.y)
}

/// The source node of a fragment.
fn node(inputs: &LayoutInputTree, fragment: &Fragment) -> NodeKey {
    inputs.items[fragment.source].node
}

/// Paint the background and border of a fragment's border box `rect`, with
/// its resolved corner radii.
fn draw_box(
    ui: &Ui,
    inputs: &LayoutInputTree,
    fragment: &Fragment,
    own: &FragmentData,
    rect: Rect,
    transform: Affine,
    scene: &mut impl PaintScene,
) {
    let Some(props) = ui.props(node(inputs, fragment)) else {
        return;
    };

    let radii = own.border_radius.unwrap_or_default();

    if let Some(fill) = props.get::<Fill>() {
        scene.fill(
            peniko::Fill::NonZero,
            transform,
            &fill.0,
            None,
            &rect.to_rounded_rect(radii),
        );
    }

    let width = border_width(fragment);

    if width > 0.0
        && let Some(fill) = props.get::<BorderFill>()
    {
        scene.stroke(
            &Stroke::new(width),
            transform,
            &fill.0,
            None,
            &rect.inset(-width / 2.0).to_rounded_rect(radii),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_glyph_run(
    tree: &FragmentTree,
    fragment: &Fragment,
    owner_line: FragmentKey,
    (line, item): (usize, usize),
    rect: Rect,
    transform: Affine,
    text_color: &anyrender::Paint,
    visible: &impl Fn(Rect) -> bool,
    scene: &mut impl PaintScene,
) {
    let Some(line) = tree
        .inline_layout(owner_line)
        .and_then(|inline| inline.layout.get(line))
    else {
        return;
    };

    let Some(PositionedLayoutItem::GlyphRun(glyph_run)) = line.items().nth(item) else {
        return;
    };

    let run = glyph_run.run();

    // Glyph ink can extend past the run's line box, such as accents above a
    // tight line height or italic overhangs. One em on each side bounds it.
    let overhang = run.font_size() as f64;

    if !visible(rect.inflate(overhang, overhang)) {
        return;
    }

    let origin = rect.origin();
    // Parley's coordinates wrap the raw `i16` bits Anyrender takes.
    let normalized_coords: &[i16] = TransparentWrapper::peel_slice(run.normalized_coords());

    // Parley's glyphs are positioned within the inline context. Offset them
    // so the run starts at the fragment's local origin.
    let offset_x = origin.x as f32 - glyph_run.offset();
    let offset_y =
        (origin.y + fragment.layout.baseline.unwrap_or_default()) as f32 - glyph_run.baseline();

    scene.draw_glyphs(
        run.font(),
        run.font_size(),
        true,
        normalized_coords,
        Vec2::ZERO,
        peniko::StyleRef::Fill(peniko::Fill::NonZero),
        text_color,
        1.0,
        transform,
        None,
        glyph_run.positioned_glyphs().map(|glyph| anyrender::Glyph {
            id: glyph.id,
            x: offset_x + glyph.x,
            y: offset_y + glyph.y,
        }),
    );
}
