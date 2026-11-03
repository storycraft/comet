pub mod prelude;
pub mod style;

use anyrender::PaintScene;
use comet_layout::{
    fragment::{Fragment, FragmentItem, FragmentKey, tree::FragmentTree},
    tree::LayoutItemKey,
};
use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};
use kurbo::{Affine, Point, Rect, Stroke};
use parley::PositionedLayoutItem;

use crate::style::{BorderFill, Fill, TextColor};

/// Paints final fragments in tree order, with coordinates relative to their parents.
#[derive(Default)]
pub struct CometRenderer {
    pending: Vec<(FragmentKey, Point, Option<LayoutItemKey>)>,
}

impl CometRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
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
                    if !fragment
                        .node
                        .and_then(|node| ui.node(node))
                        .is_some_and(|node| *node == Node::Text)
                    {
                        draw_box(ui, fragment, origin, scene);
                    }
                }
                FragmentItem::Inline { .. } => {
                    draw_box(ui, fragment, origin, scene);
                }
                FragmentItem::Line { .. } => inline_owner = Some(fragment.source),
                FragmentItem::GlyphRun { line, item } => {
                    draw_box(ui, fragment, origin, scene);

                    if let Some(owner) = inline_owner {
                        draw_glyph_run(ui, tree, fragment, owner, (line, item), origin, scene);
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

fn draw_box(ui: &Ui, fragment: &Fragment, origin: Point, scene: &mut impl PaintScene) {
    let Some(props) = fragment.node.and_then(|node| ui.props(node)) else {
        return;
    };

    let rect = Rect::from_origin_size(
        origin,
        (fragment.layout.size.width, fragment.layout.size.height),
    );

    if let Some(fill) = props.get::<Fill>() {
        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            &fill.0,
            None,
            &rect,
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
            &rect.inset(-width / 2.0),
        );
    }
}

fn draw_glyph_run(
    ui: &Ui,
    tree: &FragmentTree,
    fragment: &Fragment,
    owner: LayoutItemKey,
    (line, item): (usize, usize),
    origin: Point,
    scene: &mut impl PaintScene,
) {
    let Some(line) = tree
        .inline_layouts
        .get(&owner)
        .and_then(|layout| layout.get(line))
    else {
        return;
    };
    let Some(PositionedLayoutItem::GlyphRun(glyph_run)) = line.items().nth(item) else {
        return;
    };

    let run = glyph_run.run();
    let paint = text_color(ui, fragment.node);
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
