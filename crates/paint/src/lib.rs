pub mod prelude;
pub mod style;

use anyrender::PaintScene;
use bytemuck::TransparentWrapper;
use comet_layout::{
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKind, tree::LayoutInputTree},
    style::geometry::UnitResolveContext,
};
use comet_tree::{node::NodeKey, ui::Ui};
use kurbo::{Affine, Point, Rect, RoundedRectRadii, Stroke, Vec2};
use parley::PositionedLayoutItem;

use crate::style::{BorderFill, BorderRadius, Fill, TextColor};

/// Paints final fragments in tree order, with coordinates relative to their parents.
#[derive(Default)]
pub struct CometPainter {
    /// Fragments to paint, with their parent origin and the line containing them.
    pending: Vec<(FragmentKey, Point, Option<FragmentKey>)>,
}

impl CometPainter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Paint the fragments under `root`. `inputs` is the input tree the
    /// fragments were built from.
    ///
    /// `units` supplies the root font size and viewport resolving paint
    /// styles, matching those the input tree was built with. Its `font_size`
    /// is replaced by each item's computed font size.
    ///
    /// Fragments painting entirely outside `viewport`, in scene coordinates,
    /// are skipped. Fragments touching its edges are painted. Their descendants are still visited, since overflow is not
    /// clipped and children may paint outside their parents.
    pub fn draw(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        tree: &FragmentTree,
        root: FragmentKey,
        units: &UnitResolveContext,
        viewport: Rect,
        scene: &mut impl PaintScene,
    ) {
        self.pending.clear();
        self.pending.push((root, Point::ORIGIN, None));

        while let Some((key, parent_origin, mut inline_owner)) = self.pending.pop() {
            let Some(fragment) = tree.items.get(key) else {
                continue;
            };

            let origin = parent_origin + (fragment.layout.location.x, fragment.layout.location.y);
            let rect = Rect::from_origin_size(
                origin,
                (fragment.layout.size.width, fragment.layout.size.height),
            );
            let box_visible = || box_bounds(fragment, rect).overlaps(viewport);

            match fragment.item {
                FragmentItem::Box { .. } => {
                    // A standalone text root also owns a layout box. Its paint
                    // belongs to its glyph fragments, not to both representations.
                    if !inputs
                        .items
                        .get(fragment.source)
                        .is_some_and(|item| item.kind == LayoutInputKind::Text)
                        && box_visible()
                    {
                        draw_box(ui, inputs, tree, key, units, rect, scene);
                    }
                }
                FragmentItem::Inline { .. } => {
                    if box_visible() {
                        draw_box(ui, inputs, tree, key, units, rect, scene);
                    }
                }
                // Anonymous blocks only position their lines.
                FragmentItem::AnonymousBlock { .. } => {}
                FragmentItem::Line { .. } => inline_owner = Some(key),
                FragmentItem::GlyphRun { line, item } => {
                    if box_visible() {
                        draw_box(ui, inputs, tree, key, units, rect, scene);
                    }

                    if let Some(owner) = inline_owner {
                        draw_glyph_run(
                            ui,
                            inputs,
                            tree,
                            fragment,
                            owner,
                            (line, item),
                            rect,
                            viewport,
                            scene,
                        );
                    }
                }
            }

            self.pending.extend(
                tree.items
                    .children(key)
                    .rev()
                    .map(|child| (child, origin, inline_owner)),
            );
        }
    }
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
fn node(inputs: &LayoutInputTree, fragment: &Fragment) -> Option<NodeKey> {
    inputs.items.get(fragment.source).map(|item| item.node)
}

fn draw_box(
    ui: &Ui,
    inputs: &LayoutInputTree,
    tree: &FragmentTree,
    key: FragmentKey,
    units: &UnitResolveContext,
    rect: Rect,
    scene: &mut impl PaintScene,
) {
    let fragment = &tree.items[key];

    let Some(props) = node(inputs, fragment).and_then(|node| ui.props(node)) else {
        return;
    };

    let mut radii = RoundedRectRadii::default();

    if let Some(radius) = props.get::<BorderRadius>() {
        let mut units = units.clone();

        if let Some(item) = inputs.items.get(fragment.source) {
            units.font_size = item.text_style.span.font_size as f64;
        }

        let resolve = |value: comet_layout::style::geometry::StyleUnit| {
            value
                .resolve(&units, rect.width().min(rect.height()))
                .max(0.0)
        };

        radii = RoundedRectRadii::new(
            resolve(radius.0.left_top.y),
            resolve(radius.0.right_bottom.x),
            resolve(radius.0.right_bottom.y),
            resolve(radius.0.left_top.x),
        );
    }

    if fragment.link.prev.is_some() {
        radii.top_left = 0.0;
        radii.bottom_left = 0.0;
    }

    if fragment.link.next.is_some() {
        radii.top_right = 0.0;
        radii.bottom_right = 0.0;
    }

    if let Some(fill) = props.get::<Fill>() {
        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
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
            Affine::IDENTITY,
            &fill.0,
            None,
            &rect.inset(-width / 2.0).to_rounded_rect(radii),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_glyph_run(
    ui: &Ui,
    inputs: &LayoutInputTree,
    tree: &FragmentTree,
    fragment: &Fragment,
    owner_line: FragmentKey,
    (line, item): (usize, usize),
    rect: Rect,
    viewport: Rect,
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

    if !rect.inflate(overhang, overhang).overlaps(viewport) {
        return;
    }

    let origin = rect.origin();
    // Parley's coordinates wrap the raw `i16` bits Anyrender takes.
    let normalized_coords: &[i16] = TransparentWrapper::peel_slice(run.normalized_coords());

    // Parley's glyphs are positioned within the inline context. The fragment
    // origin already includes its line and enclosing inline spans.
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
        &text_color(ui, node(inputs, fragment)),
        1.0,
        Affine::IDENTITY,
        None,
        glyph_run.positioned_glyphs().map(|glyph| anyrender::Glyph {
            id: glyph.id,
            x: offset_x + glyph.x,
            y: offset_y + glyph.y,
        }),
    );
}

fn text_color(ui: &Ui, mut node: Option<NodeKey>) -> anyrender::Paint {
    while let Some(props) = node.and_then(|key| ui.props(key)) {
        if let Some(color) = props.get::<TextColor>() {
            return color.0.clone();
        }

        node = props.parent();
    }

    TextColor::default().0
}
