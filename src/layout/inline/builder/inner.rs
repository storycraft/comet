use kurbo::{Point, Size};
use parley::{ClusterPath, Line, PositionedLayoutItem};

use crate::{
    layout::{
        inline::{
            builder::InlineTreeBuilder,
            tree::{
                InlineNode, InlineNodeKey, InlineNodePart, InlineNodeTy, InlineTextRun, InlineTree,
            },
        },
        tree::{InlineIns, InlineLayoutNodeKey, LayoutNodeKey, LayoutTree},
    },
    ui::NodeKey,
};

pub struct Builder<'a> {
    pub cx: &'a mut InlineTreeBuilder,
    pub layout_tree: &'a LayoutTree,
    pub tree: &'a mut InlineTree,
    pub next_height: f64,
}

impl<'a> Builder<'a> {
    pub fn build(mut self, key: InlineLayoutNodeKey) -> Option<InlineNodeKey> {
        let inline_node = self.layout_tree.inline_nodes.get(key)?;

        let mut ins_iter = self
            .layout_tree
            .inlines
            .cursor(inline_node.inline_start)
            .flat_map(|key| self.layout_tree.inlines.get(key).copied());

        let mut start_line: Option<InlineNodeKey> = None;
        let mut last_line: Option<InlineNodeKey> = None;
        for line in inline_node.layout.lines() {
            self.close_unfinished_parents();
            let line_node = self.build_line(line, &mut ins_iter);

            if start_line.is_none() {
                start_line = Some(line_node);
            } else if let Some(last_line) = last_line.replace(line_node) {
                self.tree.nodes.after(last_line, line_node);
            }
        }
        // Process remaining closing instructions
        for ins in ins_iter {
            match ins {
                InlineIns::PopInlineBox => {
                    self.close_parent();
                }
                InlineIns::Node(span) => {
                    self.add_layout_node(span);
                }
                _ => {}
            }
        }
        self.close_unfinished_parents();

        start_line
    }

    fn build_line(
        &mut self,
        line: Line<'_, ()>,
        ins_iter: &mut impl Iterator<Item = InlineIns>,
    ) -> InlineNodeKey {
        let metrics = line.metrics();
        let mut inline_node = InlineNode::new(InlineNodeTy::Box(None));
        inline_node.layout.location = Point::new(metrics.offset as _, self.next_height);
        let size = Size::new(metrics.advance as _, metrics.line_height as _);
        inline_node.layout.size = size;
        inline_node.layout.content_size = size;
        self.next_height += metrics.line_height as f64;

        let line_box_key = self.tree.nodes.insert(inline_node);
        self.cx.parents.push(line_box_key);

        self.restore_unfinished_parents();
        // positioned items have offset added
        let mut offset = -metrics.offset as f64;
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                continue;
            };

            let mut length = 0usize;
            let mut cluster_iter = glyph_run.run().visual_clusters();
            while let Some(start_cluster) = cluster_iter.next() {
                let Some(text_len) = self.next_text(start_cluster.path(), offset, ins_iter) else {
                    break;
                };

                offset += start_cluster.advance() as f64;
                length += start_cluster.text_range().len();
                for cluster in &mut cluster_iter {
                    offset += cluster.advance() as f64;
                    length += cluster.text_range().len();

                    if length >= text_len {
                        let start = start_cluster.path();
                        let end = cluster.path();

                        let text_box = self.tree.nodes.insert(InlineNode::new(InlineNodeTy::Text(
                            InlineTextRun {
                                run_start_index: start.run_index(),
                                cluster_start: start.logical_index(),
                                run_end_index: end.run_index(),
                                cluster_end: end.logical_index(),
                            },
                        )));
                        self.add_child_id(text_box);
                        break;
                    }
                }
            }
        }

        line_box_key
    }

    fn next_text(
        &mut self,
        current: ClusterPath,
        offset: f64,
        iter: &mut impl Iterator<Item = InlineIns>,
    ) -> Option<usize> {
        for ins in iter {
            match ins {
                InlineIns::Text(len) => return Some(len),
                InlineIns::PushInlineBox(span) => {
                    self.cx.states.push(InlineState {
                        span,
                        start: current,
                        start_offset: offset,
                    });

                    let child = self.tree.nodes.insert(InlineNode::new_parted(
                        InlineNodeTy::Box(Some(span)),
                        InlineNodePart::Start,
                    ));
                    self.add_child_id(child);
                    self.cx.parents.push(child);
                }
                InlineIns::PopInlineBox => {
                    self.close_parent();
                }
                InlineIns::Node(span) => {
                    self.add_layout_node(span);
                }
            }
        }

        None
    }

    fn close_parent(&mut self) {
        self.cx.states.pop();
        let Some(parent_node) = self
            .cx
            .parents
            .pop()
            .and_then(|parent| self.tree.nodes.get_mut(parent))
        else {
            return;
        };

        parent_node.part = if parent_node.part == InlineNodePart::Start {
            InlineNodePart::Full
        } else {
            InlineNodePart::End
        };
    }

    fn add_layout_node(&mut self, node: LayoutNodeKey) {
        let Some(layout_node) = self.layout_tree.nodes.get(node) else {
            return;
        };
        let mut node = InlineNode::new(InlineNodeTy::LayoutNode(node));
        // TODO:: location
        node.layout.size = layout_node.layout.size;
        node.layout.content_size = layout_node.layout.content_size;

        let inline_layout_node = self.tree.nodes.insert(node);
        self.add_child_id(inline_layout_node);
    }

    fn close_unfinished_parents(&mut self) {
        self.cx.parents.clear();
    }

    fn restore_unfinished_parents(&mut self) {
        for state in &self.cx.states {
            // re-insert pending inline nodes as middle part
            let child = self.tree.nodes.insert(InlineNode::new_parted(
                InlineNodeTy::Box(Some(state.span)),
                InlineNodePart::Middle,
            ));

            let Some(parent) = self.cx.parents.last().copied() else {
                continue;
            };

            self.tree.nodes.append(parent, child);
        }
    }

    fn add_child_id(&mut self, id: InlineNodeKey) {
        let Some(parent) = self.cx.parents.last().copied() else {
            return;
        };

        self.tree.nodes.append(parent, id);
    }
}

pub struct InlineState {
    span: NodeKey,
    start: ClusterPath,
    start_offset: f64,
}
