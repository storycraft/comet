use anyrender::{Paint, PaintScene};
use color::AlphaColor;
use kurbo::{Affine, Rect, Stroke};
use parley::{
    FontContext, GenericFamily, PositionedLayoutItem,
    fontique::{SourceCache, SourceCacheOptions},
};
use peniko::{FontData, StyleRef};
use slotmap::KeyData;

use crate::layout::{InlineBoxKey, LayoutBoxKey, LayoutBoxTree, LayoutTy};

pub struct CometRenderer {
    offset_x: f64,
    offset_y: f64,
}

impl Default for CometRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl CometRenderer {
    pub fn new() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    pub fn draw(&mut self, tree: &LayoutBoxTree, id: LayoutBoxKey, scene: &mut impl PaintScene) {
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        self.draw_node(tree, id, scene);
    }

    fn draw_node(&mut self, tree: &LayoutBoxTree, id: LayoutBoxKey, scene: &mut impl PaintScene) {
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
                scene.stroke(
                    &Stroke::new(1.0),
                    Affine::IDENTITY,
                    Paint::Solid(AlphaColor::BLACK),
                    None,
                    &Rect::new(x0, y0, x1, y1),
                );

                for child_id in tree.boxes.cursor(tree.boxes.first_child(id)) {
                    self.draw_node(tree, child_id, scene);
                }
            }

            LayoutTy::Inline(inline_box_id) => {
                self.draw_inline_box(tree, inline_box_id, scene);
            }
        }

        self.offset_x = last_offset.0;
        self.offset_y = last_offset.1;
    }

    fn draw_inline_box(
        &mut self,
        tree: &LayoutBoxTree,
        id: InlineBoxKey,
        scene: &mut impl PaintScene,
    ) {
        let Some(node) = tree.inline_boxes.get(id) else {
            return;
        };

        // TODO:: move
        let mut font_cx = FontContext::new();
        let font_id = font_cx
            .collection
            .generic_families(GenericFamily::SansSerif)
            .next()
            .unwrap();
        let font_info = font_cx.collection.family(font_id).unwrap();
        let source = font_info.fonts()[0].source();
        let mut cache = SourceCache::new(SourceCacheOptions::default());
        let blob = cache.get(source).unwrap();
        let font_data = FontData::new(blob, 0);

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
                        scene.draw_glyphs(
                            &font_data,
                            16.0,
                            true,
                            &[],
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
}
