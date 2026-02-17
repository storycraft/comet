use kurbo::{Point, Size};
use parley::{Line, Run};

use crate::{
    inline::tree::{InlineNode, InlineNodeKey, InlineNodeTy, InlineTree},
    tree::LayoutTree,
};

pub struct InlineLayoutContext {}

impl InlineLayoutContext {
    pub fn new() -> Self {
        Self {}
    }

    pub fn layout(
        &mut self,
        tree: &LayoutTree,
        inline_tree: &mut InlineTree,
        parley: &parley::Layout<()>,
        line_start: InlineNodeKey,
    ) {
        let mut next_line_node_id = Some(line_start);
        for line in parley.lines() {
            let Some(line_node_id) = next_line_node_id else {
                break;
            };
            next_line_node_id = inline_tree.nodes.next_sibling(line_node_id);

            self.layout_line(tree, inline_tree, line, line_node_id);
        }
    }

    fn layout_line(
        &mut self,
        tree: &LayoutTree,
        inline_tree: &mut InlineTree,
        line: Line<'_, ()>,
        node_id: InlineNodeKey,
    ) {
        let Some(line_node) = inline_tree.nodes.get_mut(node_id) else {
            return;
        };
        let metrics = line.metrics();
        line_node.layout.location = Point::new(metrics.offset as _, metrics.min_coord as _);
        line_node.layout.size = Size::new(metrics.advance as _, metrics.line_height as _);
        line_node.layout.content_size = line_node.layout.size;

        self.layout_children(tree, inline_tree, line, node_id);
    }

    fn layout_children(
        &mut self,
        tree: &LayoutTree,
        inline_tree: &mut InlineTree,
        line: Line<'_, ()>,
        node_id: InlineNodeKey,
    ) {
        let metrics = line.metrics();
        let mut next_child = inline_tree.nodes.first_child(node_id);

        let mut offset = 0.0f64;
        while let Some(child) = next_child {
            next_child = inline_tree.nodes.next_sibling(child);

            // TODO:: cleanup code
            if let Some(InlineNode {
                ty: InlineNodeTy::Box(_),
                ..
            }) = inline_tree.nodes.get(child)
            {
                self.layout_children(tree, inline_tree, line, child);
            }

            let Some(node) = inline_tree.nodes.get_mut(child) else {
                continue;
            };

            // TODO:: location
            match node.ty {
                InlineNodeTy::LayoutNode(layout_node_key) => {
                    let Some(layout_node) = tree.nodes.get(layout_node_key) else {
                        continue;
                    };

                    // TODO
                    node.layout = layout_node.layout.clone();
                }
                InlineNodeTy::Box(_) => {
                    // TODO:: calculate total children bounding box
                    // node.layout.content_size = node.layout.size;
                }
                InlineNodeTy::Text(inline_run) => {
                    let Some(run) = line.runs().nth(inline_run.run_index) else {
                        continue;
                    };

                    node.layout.size = Size::new(
                        self.calc_run_advance(
                            run,
                            inline_run.cluster_start..=inline_run.cluster_end,
                        ),
                        metrics.line_height as _,
                    );
                    node.layout.content_size = node.layout.size;
                }
            }

            // TODO:: bidi, baseline alignment
            node.layout.location = Point::new(offset, 0.0);
            offset += node.layout.size.width;
        }
    }

    fn calc_run_advance(&mut self, run: Run<()>, range: impl IntoIterator<Item = usize>) -> f64 {
        let mut total_advance = 0.0f64;
        for index in range {
            let Some(cluster) = run.get(index) else {
                break;
            };

            total_advance += cluster.advance() as f64;
        }

        total_advance
    }
}
