use anyrender::{Paint, PaintScene};
use color::AlphaColor;
use kurbo::{Affine, Rect, RoundedRect};
use parley::PositionedLayoutItem;
use peniko::{Fill, StyleRef};
use slotmap::KeyData;

use crate::{
    Ui,
    layout::tree::{InlineBoxKey, LayoutBoxKey, LayoutBoxTree, LayoutTy},
    node::{Node, NodeKey},
};

pub struct CometRenderer {
    offset_x: f64,
    offset_y: f64,
}

impl CometRenderer {
    pub fn new() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
        tree: &LayoutBoxTree,
        id: LayoutBoxKey,
        scene: &mut impl PaintScene,
    ) {
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        self.draw_node(ui, tree, id, scene);
    }

    fn draw_node(
        &mut self,
        ui: &Ui,
        tree: &LayoutBoxTree,
        id: LayoutBoxKey,
        scene: &mut impl PaintScene,
    ) {
        let Some(node) = tree.boxes.get(id) else {
            return;
        };

        let location = node.taffy_layout.location;
        let last_offset = (self.offset_x, self.offset_y);
        self.offset_x += location.x as f64;
        self.offset_y += location.y as f64;

        match node.ty {
            LayoutTy::Block => {
                let size = node.taffy_layout.size;
                let x0 = self.offset_x;
                let y0 = self.offset_y;
                let x1 = x0 + size.width as f64;
                let y1 = y0 + size.height as f64;

                if let Some(span) = node.span {
                    self.draw_block(ui, span, Rect::new(x0, y0, x1, y1), scene);
                }

                for child_id in tree.boxes.cursor(tree.boxes.first_child(id)) {
                    self.draw_node(ui, tree, child_id, scene);
                }
            }

            LayoutTy::Inline(inline_box_id) => {
                self.draw_inline_box(ui, tree, inline_box_id, scene);
            }
        }

        self.offset_x = last_offset.0;
        self.offset_y = last_offset.1;
    }

    fn draw_inline_box(
        &mut self,
        ui: &Ui,
        tree: &LayoutBoxTree,
        id: InlineBoxKey,
        scene: &mut impl PaintScene,
    ) {
        let Some(node) = tree.inline_boxes.get(id) else {
            return;
        };

        for line in node.parley_layout.lines() {
            // TODO:: proper height calc
            let mut height_offset = 0.0f32;
            for item in line.items() {
                let PositionedLayoutItem::InlineBox(inline_box) = item else {
                    continue;
                };

                if inline_box.y < 0.0 {
                    height_offset = height_offset.max(-inline_box.y);
                }
            }

            for item in line.items() {
                match item {
                    PositionedLayoutItem::GlyphRun(glyph_run) => {
                        let run = glyph_run.run();
                        scene.draw_glyphs(
                            &run.font(),
                            run.font_size(),
                            true,
                            run.normalized_coords(),
                            StyleRef::Fill(peniko::Fill::NonZero),
                            Paint::Solid(AlphaColor::BLACK),
                            1.0,
                            Affine::IDENTITY,
                            None,
                            glyph_run.positioned_glyphs().map(|glyph| anyrender::Glyph {
                                id: glyph.id,
                                x: self.offset_x as f32 + glyph.x,
                                y: self.offset_y as f32 + glyph.y - height_offset,
                            }),
                        );
                    }
                    PositionedLayoutItem::InlineBox(inline_box) => {
                        let last_offset = (self.offset_x, self.offset_y);
                        self.offset_x += inline_box.x as f64;
                        self.offset_y += inline_box.y as f64 + height_offset as f64;

                        self.draw_node(
                            ui,
                            tree,
                            LayoutBoxKey::from(KeyData::from_ffi(inline_box.id)),
                            scene,
                        );

                        self.offset_x = last_offset.0;
                        self.offset_y = last_offset.1;
                    }
                };
            }
        }
    }

    fn draw_block(&self, ui: &Ui, id: NodeKey, rect: Rect, scene: &mut impl PaintScene) {
        let Some(Node::Div(div)) = ui.elements.get(id) else {
            return;
        };

        let rect = RoundedRect::from_rect(rect, div.border_radius);
        if let Some(fill) = &div.fill {
            scene.fill(Fill::EvenOdd, div.transform, fill, None, &rect);
        }

        if let Some((stroke_style, stroke_paint)) = &div.stroke {
            scene.stroke(stroke_style, div.transform, stroke_paint, None, &rect);
        }
    }
}

impl Default for CometRenderer {
    fn default() -> Self {
        Self::new()
    }
}
