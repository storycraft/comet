use slotmap::SecondaryMap;

use crate::{
    layout::input::{InlineIns, InlineKey, InlineNode, InputNode, InputNodeKey, LayoutInputTree},
    style::div::{DisplayInner, DisplayOuter},
    ui::{Node, NodeKey, Ui},
};

pub struct LayoutInputTreeBuilderContext {
    parents: Vec<InputNodeKey>,
    inline: Option<(InlineNode, InlineKey)>,
    map: SecondaryMap<InputNodeKey, NodeKey>,
}

impl LayoutInputTreeBuilderContext {
    pub fn new() -> Self {
        Self {
            parents: vec![],
            inline: None,
            map: SecondaryMap::new(),
        }
    }

    pub fn builder<'a>(
        &'a mut self,
        ui: &'a Ui,
        tree: &'a mut LayoutInputTree,
    ) -> LayoutInputTreeBuilder<'a> {
        LayoutInputTreeBuilder { cx: self, ui, tree }
    }
}

impl Default for LayoutInputTreeBuilderContext {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LayoutInputTreeBuilder<'a> {
    cx: &'a mut LayoutInputTreeBuilderContext,
    ui: &'a Ui,
    tree: &'a mut LayoutInputTree,
}

impl LayoutInputTreeBuilder<'_> {
    /// Build a node into layout boxes and append inside box_id
    pub fn build(&mut self, id: NodeKey, input_node_id: InputNodeKey) {
        self.cx.parents.push(input_node_id);
        self.build_siblings(Some(id), false);
        // commit remaining inlines
        self.commit_inlines();
        self.cx.parents.clear();
    }

    fn build_siblings(&mut self, start: Option<NodeKey>, force_block: bool) {
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

            match (node.as_deref(), force_block, display_outer) {
                (Some(Node::Text(_)), _, _) | (_, false, DisplayOuter::Inline) => {
                    if display_inner == DisplayInner::Flow {
                        self.push_inline(InlineIns::PushInlineBox(id));
                        self.build_inner(id, display_inner);
                        self.push_inline(InlineIns::PopInlineBox);
                    } else {
                        self.build_inner(id, display_inner);
                    }
                }

                (_, _, DisplayOuter::Block) | (_, true, _) => {
                    self.commit_inlines();
                    let block_node_id = self.add_child(Some(id), InputNode::Block);
                    self.cx.parents.push(block_node_id);
                    self.build_inner(id, display_inner);
                    self.commit_inlines();
                    self.cx.parents.pop();
                }
            }
        }
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
                    self.push_inline(InlineIns::Text(text.len()));
                    // TODO
                    self.cx.inline.as_mut().unwrap().0.texts.push_str(text);
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
                let last_inline = self.cx.inline.take();
                let block_node_id = self.tree.nodes.insert(InputNode::Block);
                self.cx.parents.push(block_node_id);

                self.build_siblings(self.ui.first_child(id), true);

                self.commit_inlines();
                self.cx.inline = last_inline;
                let input_node_id = self.cx.parents.pop().unwrap();
                self.push_inline(InlineIns::Node(input_node_id));
            }

            DisplayInner::Container(_) => {
                self.build_siblings(self.ui.first_child(id), true);
            }

            DisplayInner::Content => {}
        }
    }

    fn push_inline(&mut self, item: InlineIns) {
        let key = self.tree.inlines.insert(item);
        match self.cx.inline {
            Some((_, ref mut last)) => {
                self.tree.inlines.after(*last, key);
                *last = key;
            }
            None => {
                self.cx.inline = Some((InlineNode::new(Some(key)), key));
            }
        }
    }

    fn commit_inlines(&mut self) {
        let Some((inline_box, _)) = self.cx.inline.take() else {
            return;
        };
        let key = self.tree.nodes.insert(InputNode::Inline(inline_box));
        self.add_child_id(None, key);
    }

    fn add_child(&mut self, span: Option<NodeKey>, node: InputNode) -> InputNodeKey {
        let id = self.tree.nodes.insert(node);
        self.add_child_id(span, id);
        id
    }

    fn add_child_id(&mut self, span: Option<NodeKey>, id: InputNodeKey) {
        let Some(parent) = self.cx.parents.last().copied() else {
            return;
        };

        let Some(parent_node) = self.tree.nodes.get_mut(parent) else {
            return;
        };

        match parent_node {
            InputNode::Block => {
                self.tree.nodes.append(parent, id);
            }

            InputNode::Inline(inline_node) => {
                let Some(item_start) = inline_node.inline_start else {
                    return;
                };

                let inline_id = self.tree.inlines.insert(InlineIns::Node(id));
                self.tree.inlines.after(item_start, inline_id);
            }
        }

        if let Some(span) = span {
            self.cx.map.insert(id, span);
        }
    }
}
