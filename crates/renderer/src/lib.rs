pub mod prelude;
pub mod style;

use anyrender::{Paint, PaintScene};
use color::AlphaColor;
use comet_div::ui::{NodeKey, Ui};
use kurbo::{Affine, Rect, RoundedRect, Stroke};
use parley::{Cluster, ClusterPath, PositionedLayoutItem};
use peniko::StyleRef;
use slotmap::KeyData;

use comet_layout::tree::{
    LayoutTree, {InlineIns, InlineLayoutNodeKey, LayoutNodeKey, LayoutNodeTy},
};

use crate::style::{BorderFill, Fill};

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
        tree: &LayoutTree,
        id: LayoutNodeKey,
        scene: &mut impl PaintScene,
    ) {
        self.offset_x = 0.0;
        self.offset_y = 0.0;
        self.draw_node(ui, tree, id, scene);
    }

    fn draw_node(
        &mut self,
        ui: &Ui,
        tree: &LayoutTree,
        id: LayoutNodeKey,
        scene: &mut impl PaintScene,
    ) {
        let Some(node) = tree.nodes.get(id) else {
            return;
        };

        let location = node.layout.location;
        let last_offset = (self.offset_x, self.offset_y);
        self.offset_x += location.x;
        self.offset_y += location.y;

        match node.ty {
            LayoutNodeTy::Block(span) => {
                let size = node.layout.size;
                let x0 = self.offset_x;
                let y0 = self.offset_y;
                let x1 = x0 + size.width;
                let y1 = y0 + size.height;

                if let Some(span) = span {
                    self.draw_block(ui, span, Rect::new(x0, y0, x1, y1), scene);
                }

                let next_start = self.inline_states.len();
                self.inline_start.push(next_start);

                for child_id in tree.nodes.cursor(tree.nodes.first_child(id)) {
                    self.draw_node(ui, tree, child_id, scene);
                }

                self.inline_start.pop();
            }

            LayoutNodeTy::Inline(inline_node_id) => {
                self.draw_inline_background(ui, tree, inline_node_id, scene);
                self.draw_inline_box(ui, tree, inline_node_id, scene);
            }
        }

        self.offset_x = last_offset.0;
        self.offset_y = last_offset.1;
    }

    fn draw_inline_state(
        &mut self,
        inline_state: &InlineState,
        ui: &Ui,
        to: ClusterPath,
        end_inline: InlineLayoutNodeKey,
        layout: &parley::Layout<Option<NodeKey>>,
        scene: &mut impl PaintScene,
    ) {
        let Some(props) = ui.props(inline_state.span) else {
            return;
        };

        let Some(start_cluster) = inline_state.start.cluster(layout) else {
            return;
        };
        let start_line = start_cluster.line();
        let start_line_metrics = start_line.metrics();

        let Some(end_cluster) = to.cluster(layout) else {
            return;
        };
        let end_line = end_cluster.line();

        if inline_state.start.line_index() == to.line_index() {
            if let Some(fill) = props.get::<Fill>() {
                let x0 = self.offset_x + start_cluster.visual_offset().unwrap_or_default() as f64;
                let y0 = self.offset_y + start_line_metrics.min_coord as f64;
                let x1 = self.offset_x
                    + end_cluster.visual_offset().unwrap_or_default() as f64
                    + end_cluster.advance() as f64;
                let y1 = self.offset_y + start_line_metrics.max_coord as f64;

                scene.fill(
                    peniko::Fill::EvenOdd,
                    Affine::IDENTITY,
                    &fill.0,
                    None,
                    &Rect::new(x0, y0, x1, y1),
                );
            }

            return;
        }

        if let Some(fill) = props.get::<Fill>() {
            let start_line_metrics = start_line.metrics();
            scene.fill(
                peniko::Fill::EvenOdd,
                Affine::IDENTITY,
                &fill.0,
                None,
                &Rect::new(
                    self.offset_x + start_cluster.visual_offset().unwrap_or_default() as f64,
                    self.offset_y + start_line.metrics().min_coord as f64,
                    self.offset_x
                        + (start_line_metrics.advance - start_line_metrics.trailing_whitespace)
                            as f64,
                    self.offset_y + start_line.metrics().max_coord as f64,
                ),
            );

            for line_index in inline_state.start.line_index()..to.line_index() {
                let Some(line) = layout.get(line_index) else {
                    break;
                };
                let metrics = line.metrics();

                scene.fill(
                    peniko::Fill::EvenOdd,
                    Affine::IDENTITY,
                    &fill.0,
                    None,
                    &Rect::new(
                        self.offset_x,
                        self.offset_y + metrics.min_coord as f64,
                        self.offset_x + (metrics.advance - metrics.trailing_whitespace) as f64,
                        self.offset_y + metrics.max_coord as f64,
                    ),
                );
            }

            scene.fill(
                peniko::Fill::EvenOdd,
                Affine::IDENTITY,
                &fill.0,
                None,
                &Rect::new(
                    self.offset_x,
                    self.offset_y + end_line.metrics().min_coord as f64,
                    self.offset_x
                        + end_cluster.visual_offset().unwrap_or_default() as f64
                        + end_cluster.advance() as f64,
                    self.offset_y + end_line.metrics().max_coord as f64,
                ),
            );
        }
    }

    // TODO:: optimize render path
    fn draw_inline_background(
        &mut self,
        ui: &Ui,
        tree: &LayoutTree,
        id: InlineLayoutNodeKey,
        scene: &mut impl PaintScene,
    ) {
        let Some(node) = tree.inline_nodes.get(id) else {
            return;
        };

        let mut text_index = 0;
        let mut next_inline = node.inline_start;
        while let Some(inline) = next_inline {
            next_inline = tree.inlines.next_sibling(inline);
            let Some(ins) = tree.inlines.get(inline) else {
                continue;
            };

            match *ins {
                InlineIns::Text(length) => {
                    text_index += length;
                }

                InlineIns::PushInline(span) => {
                    let Some(cluster) = Cluster::from_byte_index(&node.layout, text_index) else {
                        continue;
                    };
                    self.inline_states.push(InlineState {
                        start: cluster.path(),
                        start_inline: id,
                        span,
                    });
                }

                InlineIns::PopInline => {
                    let Some(inline_state) = self.inline_states.pop() else {
                        continue;
                    };
                    let Some(cluster) = Cluster::from_byte_index(&node.layout, text_index - 1)
                    else {
                        continue;
                    };

                    self.draw_inline_state(
                        &inline_state,
                        ui,
                        cluster.path(),
                        id,
                        &node.layout,
                        scene,
                    );
                }

                InlineIns::Node(_) => {}
            }
        }
    }

    fn draw_inline_box(
        &mut self,
        ui: &Ui,
        tree: &LayoutTree,
        id: InlineLayoutNodeKey,
        scene: &mut impl PaintScene,
    ) {
        let Some(node) = tree.inline_nodes.get(id) else {
            return;
        };

        for line in node.layout.lines() {
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
                            LayoutNodeKey::from(KeyData::from_ffi(inline_box.id)),
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
    pub start: ClusterPath,
    pub start_inline: InlineLayoutNodeKey,
    pub span: NodeKey,
}
