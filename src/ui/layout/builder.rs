pub mod inline;

use crate::{
    layout::tree::{
        LayoutTree,
        node::{InlineIns, LayoutNode, LayoutNodeKey, LayoutNodeTy},
    },
    style::div::{DisplayInner, DisplayOuter},
    ui::{Node, NodeKey, Ui, layout::UiLayoutBuilder},
};

pub struct Builder<'a> {
    pub cx: &'a mut UiLayoutBuilder,
    pub ui: &'a Ui,
    pub tree: &'a mut LayoutTree,
}

impl Builder<'_> {
    /// Build siblings of [`Node`] inside given [`LayoutNode`]
    pub fn build(mut self, id: NodeKey, input_node_id: LayoutNodeKey) {
        self.cx.parents.push(input_node_id);
        self.build_siblings(Some(id), false);
        // commit remaining inlines
        self.commit_inlines();
        self.cx.parents.clear();
    }

    fn build_inner(&mut self, id: NodeKey, inner: DisplayInner) {
        let Some(node) = self.ui.node(id) else {
            return;
        };

        match *node {
            Node::Div => {
                self.build_div(id, inner);
            }
            Node::Text(ref text) => {
                if !text.is_empty() {
                    self.cx
                        .inline
                        .add_ins(self.tree, InlineIns::Text(text.len()));
                    self.cx.inline.add_span(id);
                    self.cx.inline.add_text(text);
                }
            }
        }
    }

    fn build_siblings(&mut self, start: Option<NodeKey>, container: bool) {
        for id in self.ui.cursor(start) {
            let node = self.ui.node(id);
            let display_outer = self
                .ui
                .prop::<DisplayOuter>(id)
                .as_deref()
                .cloned()
                .unwrap_or_default();

            let display_inner = self
                .ui
                .prop::<DisplayInner>(id)
                .as_deref()
                .cloned()
                .unwrap_or_default();

            match (node.as_deref(), container, display_outer) {
                (Some(Node::Text(_)), _, _) | (_, false, DisplayOuter::Inline) => {
                    if display_inner == DisplayInner::Flow {
                        self.cx
                            .inline
                            .add_ins(self.tree, InlineIns::PushInlineBox(id));
                        self.cx.inline.add_span(id);
                        self.build_inner(id, display_inner);
                        self.cx.inline.add_ins(self.tree, InlineIns::PopInlineBox);
                    } else {
                        self.build_inner(id, display_inner);
                    }
                }

                (_, _, DisplayOuter::Block) | (_, true, _) => {
                    self.commit_inlines();
                    let layout_box_id =
                        self.add_child(LayoutNode::new(LayoutNodeTy::Block(Some(id))));
                    self.cx.parents.push(layout_box_id);
                    self.build_inner(id, display_inner);
                    self.commit_inlines();
                    self.cx.parents.pop();
                }
            }
        }
    }

    fn build_div(&mut self, id: NodeKey, inner: DisplayInner) {
        match inner {
            DisplayInner::Flow => {
                self.build_siblings(self.ui.first_child(id), false);
            }

            DisplayInner::FlowRoot => {
                self.cx.inline.push();
                let block_node_id = self
                    .tree
                    .nodes
                    .insert(LayoutNode::new(LayoutNodeTy::Block(Some(id))));
                self.cx.parents.push(block_node_id);

                self.build_siblings(self.ui.first_child(id), false);

                self.commit_inlines();
                let input_node_id = self.cx.parents.pop().unwrap();
                self.cx
                    .inline
                    .add_ins(self.tree, InlineIns::Node(input_node_id));
            }

            DisplayInner::Container(_) => {
                self.build_siblings(self.ui.first_child(id), true);
            }

            DisplayInner::Content => {}
        }
    }

    fn add_child(&mut self, node: LayoutNode) -> LayoutNodeKey {
        let id = self.tree.nodes.insert(node);
        self.add_child_id(id);
        id
    }

    fn add_child_id(&mut self, id: LayoutNodeKey) {
        let Some(parent) = self.cx.parents.last().copied() else {
            return;
        };

        let Some(parent_node) = self.tree.nodes.get_mut(parent) else {
            return;
        };

        match parent_node.ty {
            LayoutNodeTy::Block(span) => {
                if let Some(span) = span {
                    self.cx.mappings.insert(span.id(), id);
                }
                self.tree.nodes.append(parent, id);
            }

            LayoutNodeTy::Inline(_) => {}
        }
    }

    fn commit_inlines(&mut self) {
        let Some((inline_node, mappings)) = self.cx.inline.commit() else {
            return;
        };

        let inline_input_key = self.tree.inline_nodes.insert(inline_node);
        let key = self
            .tree
            .nodes
            .insert(LayoutNode::new(LayoutNodeTy::Inline(inline_input_key)));
        for span in mappings {
            self.cx.mappings.insert(span.id(), key);
        }
        self.add_child_id(key);
    }
}
