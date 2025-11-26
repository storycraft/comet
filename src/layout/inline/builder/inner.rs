use core::iter::Peekable;

use parley::{Cluster, Line};

use crate::layout::{
    inline::{
        builder::InlineTreeBuilder,
        clusters::LineClusters,
        tree::{
            InlineNode, InlineNodeKey, InlineNodePart, InlineNodeTy, InlineTextRun, InlineTree,
        },
    },
    tree::{InlineIns, InlineLayoutNodeKey, LayoutNodeKey, LayoutTree},
};

pub struct Builder<'a> {
    pub cx: &'a mut InlineTreeBuilder,
    pub layout_tree: &'a LayoutTree,
    pub tree: &'a mut InlineTree,
}

impl<'a> Builder<'a> {
    pub fn build(self, key: InlineLayoutNodeKey) -> Option<InlineNodeKey> {
        let inline_node = self.layout_tree.inline_nodes.get(key)?;

        let mut ins_iter = self
            .layout_tree
            .inlines
            .cursor(inline_node.inline_start)
            .flat_map(|key| self.layout_tree.inlines.get(key).copied())
            .peekable();

        let mut start_line: Option<InlineNodeKey> = None;
        let mut last_line: Option<InlineNodeKey> = None;
        for line in inline_node.layout.lines() {
            let line_node = LineBuilder {
                cx: self.cx,
                line,
                ins_iter: &mut ins_iter,
                tree: self.tree,
            }
            .build();

            if start_line.is_none() {
                start_line = Some(line_node);
                last_line = Some(line_node);
            } else if let Some(last_line) = last_line.replace(line_node) {
                self.tree.nodes.after(last_line, line_node);
            }
        }

        start_line
    }
}

struct LineBuilder<'a, I: Iterator> {
    pub cx: &'a mut InlineTreeBuilder,
    pub line: Line<'a, ()>,
    pub ins_iter: &'a mut Peekable<I>,
    pub tree: &'a mut InlineTree,
}

impl<'a, I> LineBuilder<'a, I>
where
    I: Iterator<Item = InlineIns>,
{
    pub fn build(mut self) -> InlineNodeKey {
        let inline_node = InlineNode::new(InlineNodeTy::Box(None));

        let line_box_key = self.tree.nodes.insert(inline_node);
        self.cx.parents.push(line_box_key);
        self.restore_unfinished_parents();

        let mut line_clusters = LineClusters::new(self.line.runs()).peekable();
        while let Some(ins) = self.ins_iter.peek().copied() {
            if self.process_ins(ins, &mut line_clusters) {
                _ = self.ins_iter.next();
                continue;
            }

            if line_clusters.peek().is_none() {
                break;
            }
        }

        line_box_key
    }

    fn process_ins<'b>(
        &mut self,
        ins: InlineIns,
        clusters: &mut Peekable<impl Iterator<Item = Cluster<'b, ()>>>,
    ) -> bool {
        match ins {
            InlineIns::Text(len) => {
                self.cx.stack.add_texts(len);
            }
            InlineIns::PushInlineBox(span) => {
                self.cx.stack.push_state(span);

                let child = self.tree.nodes.insert(InlineNode::new_parted(
                    InlineNodeTy::Box(Some(span)),
                    InlineNodePart::Start,
                ));
                self.add_child_id(child);
                self.cx.parents.push(child);
            }
            InlineIns::PopInlineBox => {
                let Some((start, end, ended)) = self.cx.stack.read(clusters) else {
                    self.close_parent();
                    return true;
                };

                let run = self.tree.nodes.insert(InlineNode::new_parted(
                    InlineNodeTy::Text(InlineTextRun {
                        run_start_index: start.run_index(),
                        cluster_start: start.logical_index(),
                        run_end_index: end.run_index(),
                        cluster_end: end.logical_index(),
                    }),
                    InlineNodePart::Full,
                ));
                self.add_child_id(run);

                if ended {
                    self.close_parent();
                } else {
                    return false;
                }
            }
            InlineIns::Node(span) => {
                self.add_layout_node(span);
            }
        }

        true
    }

    fn close_parent(&mut self) {
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
        let inline_layout_node = self
            .tree
            .nodes
            .insert(InlineNode::new(InlineNodeTy::LayoutNode(node)));
        self.add_child_id(inline_layout_node);
    }

    fn restore_unfinished_parents(&mut self) {
        for span in self.cx.stack.spans() {
            // re-insert pending inline nodes as middle part
            let child = self.tree.nodes.insert(InlineNode::new_parted(
                InlineNodeTy::Box(Some(span)),
                InlineNodePart::Middle,
            ));

            let last_parent = self.cx.parents.last().copied();
            self.cx.parents.push(child);
            let Some(last_parent) = last_parent else {
                continue;
            };

            self.tree.nodes.append(last_parent, child);
        }
    }

    fn add_child_id(&mut self, id: InlineNodeKey) {
        let Some(parent) = self.cx.parents.last().copied() else {
            return;
        };

        self.tree.nodes.append(parent, id);
    }
}
