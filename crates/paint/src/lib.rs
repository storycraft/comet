pub mod prelude;
pub mod style;

use anyrender::PaintScene;
use comet_layout::{
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKind, tree::LayoutInputTree},
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
    pub fn draw(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        tree: &FragmentTree,
        root: FragmentKey,
        scene: &mut impl PaintScene,
    ) {
        self.pending.clear();
        self.pending.push((root, Point::ORIGIN, None));

        while let Some((key, parent_origin, mut inline_owner)) = self.pending.pop() {
            let Some(fragment) = tree.items.get(key) else {
                continue;
            };

            let origin = parent_origin + (fragment.layout.location.x, fragment.layout.location.y);

            match fragment.item {
                FragmentItem::Box => {
                    // A standalone text root also owns a layout box. Its paint
                    // belongs to its glyph fragments, not to both representations.
                    if !inputs
                        .items
                        .get(fragment.source)
                        .is_some_and(|item| item.kind == LayoutInputKind::Text)
                    {
                        draw_box(ui, inputs, tree, key, origin, scene);
                    }
                }
                FragmentItem::Inline { .. } => {
                    draw_box(ui, inputs, tree, key, origin, scene);
                }
                // Anonymous blocks only position their lines.
                FragmentItem::AnonymousBlock => {}
                FragmentItem::Line { .. } => inline_owner = Some(key),
                FragmentItem::GlyphRun { line, item } => {
                    draw_box(ui, inputs, tree, key, origin, scene);

                    if let Some(owner) = inline_owner {
                        draw_glyph_run(
                            tree,
                            fragment,
                            text_color(ui, node(inputs, fragment)),
                            owner,
                            (line, item),
                            origin,
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

/// The source node of a fragment.
fn node(inputs: &LayoutInputTree, fragment: &Fragment) -> Option<NodeKey> {
    inputs.items.get(fragment.source).map(|item| item.node)
}

fn draw_box(
    ui: &Ui,
    inputs: &LayoutInputTree,
    tree: &FragmentTree,
    key: FragmentKey,
    origin: Point,
    scene: &mut impl PaintScene,
) {
    let fragment = &tree.items[key];

    let Some(props) = node(inputs, fragment).and_then(|node| ui.props(node)) else {
        return;
    };

    let rect = Rect::from_origin_size(
        origin,
        (fragment.layout.size.width, fragment.layout.size.height),
    );
    let mut radii = RoundedRectRadii::default();

    if let Some(radius) = props.get::<BorderRadius>()
        && let Some(constraint) = tree.layout_constraint(key)
    {
        let mut units = constraint.units.clone();

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

    let border = fragment.layout.border;
    let width = border
        .left_top
        .x
        .max(border.left_top.y)
        .max(border.right_bottom.x)
        .max(border.right_bottom.y);

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

fn draw_glyph_run(
    tree: &FragmentTree,
    fragment: &Fragment,
    paint: anyrender::Paint,
    owner_line: FragmentKey,
    (line, item): (usize, usize),
    origin: Point,
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
    let normalized_coords: Vec<_> = run
        .normalized_coords()
        .iter()
        .map(|coord| coord.to_bits())
        .collect();

    // Parley's glyphs are positioned within the inline context. The fragment
    // origin already includes its line and enclosing inline spans.
    let offset_x = origin.x as f32 - glyph_run.offset();
    let offset_y =
        (origin.y + fragment.layout.baseline.unwrap_or_default()) as f32 - glyph_run.baseline();

    scene.draw_glyphs(
        run.font(),
        run.font_size(),
        true,
        &normalized_coords,
        Vec2::ZERO,
        peniko::StyleRef::Fill(peniko::Fill::NonZero),
        &paint,
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
    while let Some(key) = node {
        if let Some(color) = ui.prop::<TextColor>(key) {
            return color.0.clone();
        }

        node = ui.parent(key);
    }

    TextColor::default().0
}
