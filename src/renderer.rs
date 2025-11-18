use anyrender::{Paint, PaintScene};
use color::AlphaColor;
use kurbo::{Affine, Rect, RoundedRect, Stroke};
use parley::{Line, LineMetrics, PositionedLayoutItem};
use peniko::StyleRef;
use slotmap::KeyData;

use crate::{
    layout::{InlineBoxKey, InlineIns, InlineKey, LayoutBoxKey, LayoutTy, tree::LayoutBoxTree},
    style::div::{BorderFill, Fill},
    ui::{NodeKey, Ui},
};

pub struct CometRenderer {
    offset_x: f64,
    offset_y: f64,
    inline_states: Vec<InlineState>,
    inline_start: Vec<usize>,
}

impl CometRenderer {
    pub fn new() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            inline_states: vec![],
            inline_start: vec![],
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

        let location = node.layout.location;
        let last_offset = (self.offset_x, self.offset_y);
        self.offset_x += location.x;
        self.offset_y += location.y;

        match node.ty {
            LayoutTy::Block => {
                let size = node.layout.size;
                let x0 = self.offset_x;
                let y0 = self.offset_y;
                let x1 = x0 + size.width;
                let y1 = y0 + size.height;

                if let Some(span) = node.span {
                    self.draw_block(ui, span, Rect::new(x0, y0, x1, y1), scene);
                }

                let next_start = self.inline_states.len();
                self.inline_start.push(next_start);

                for child_id in tree.boxes.cursor(tree.boxes.first_child(id)) {
                    self.draw_node(ui, tree, child_id, scene);
                }

                self.inline_start.pop();
            }

            LayoutTy::Inline(inline_box_id) => {
                self.draw_inline_background(ui, tree, inline_box_id, scene);
                self.draw_inline_box(ui, tree, inline_box_id, scene);
            }
        }

        self.offset_x = last_offset.0;
        self.offset_y = last_offset.1;
    }

    fn draw_inline_state(
        &mut self,
        inline_state: &InlineState,
        ui: &Ui,
        line_index: usize,
        metrics: &LineMetrics,
        scene: &mut impl PaintScene,
    ) {
        let offset = if inline_state.start_line_index == line_index {
            inline_state.start_line_offset
        } else {
            0.0
        };

        let Some(props) = ui.props(inline_state.span) else {
            return;
        };

        if let Some(fill) = props.get::<Fill>() {
            let x0 = self.offset_x as f64 + offset as f64;
            let y0 = self.offset_y as f64 + metrics.min_coord as f64;
            let x1 = self.offset_x as f64 + (metrics.advance - metrics.trailing_whitespace) as f64;
            let y1 = self.offset_y as f64 + metrics.max_coord as f64;
            scene.fill(
                peniko::Fill::EvenOdd,
                Affine::IDENTITY,
                &fill.0,
                None,
                &Rect::new(x0, y0, x1, y1),
            );
        }
    }

    fn draw_inline_until(
        &mut self,
        ui: &Ui,
        tree: &LayoutBoxTree,
        start: Option<InlineKey>,
        span: NodeKey,
        line_index: usize,
        line: Line<NodeKey>,
        offset: f32,
        scene: &mut impl PaintScene,
    ) -> Option<InlineKey> {
        let mut next_inline = start;
        while let Some(inline) = next_inline {
            next_inline = tree.inlines.next_sibling(inline);
            let Some(ins) = tree.inlines.get(inline) else {
                continue;
            };

            match *ins {
                InlineIns::Text(key) => {
                    if span == key {
                        return Some(inline);
                    }
                }
                InlineIns::PushInlineBox(span) => {
                    self.inline_states.push(InlineState {
                        start_line_index: line_index,
                        start_line_offset: offset,
                        span,
                    });
                }
                InlineIns::PopInlineBox => {
                    let Some(inline_state) = self.inline_states.pop() else {
                        continue;
                    };

                    self.draw_inline_state(&inline_state, ui, line_index, line.metrics(), scene);
                }
                _ => {}
            }
        }

        None
    }

    // TODO:: optimize render path
    fn draw_inline_background(
        &mut self,
        ui: &Ui,
        tree: &LayoutBoxTree,
        id: InlineBoxKey,
        scene: &mut impl PaintScene,
    ) {
        let Some(node) = tree.inline_boxes.get(id) else {
            return;
        };

        let mut next_inline = node.inline_start;
        for (line_index, line) in node.parley_layout.lines().enumerate() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };

                next_inline = self.draw_inline_until(
                    ui,
                    tree,
                    next_inline,
                    glyph_run.style().brush,
                    line_index,
                    line,
                    glyph_run.offset(),
                    scene,
                );
            }

            // TODO
            for inline_state in self
                .inline_states
                .clone()
                .iter()
                .skip(self.inline_start.last().copied().unwrap_or(0))
            {
                self.draw_inline_state(inline_state, ui, line_index, line.metrics(), scene);
            }
        }
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
                            run.font(),
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

    // TODO::
    fn draw_block(&self, ui: &Ui, id: NodeKey, rect: Rect, scene: &mut impl PaintScene) {
        let Some(props) = ui.props(id) else {
            return;
        };
        let rect = RoundedRect::from_rect(rect, 0.0);

        if let Some(fill) = props.get::<Fill>() {
            scene.fill(
                peniko::Fill::EvenOdd,
                Affine::IDENTITY,
                &fill.0,
                None,
                &rect,
            );
        }

        if let Some(stroke_paint) = props.get::<BorderFill>() {
            scene.stroke(
                &Stroke::default(),
                Affine::IDENTITY,
                &stroke_paint.0,
                None,
                &rect,
            );
        }
    }
}

impl Default for CometRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
struct InlineState {
    pub start_line_index: usize,
    pub start_line_offset: f32,
    pub span: NodeKey,
}
