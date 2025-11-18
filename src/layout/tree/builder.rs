use crate::{
    layout::{
        InlineBox, InlineIns, InlineKey, LayoutBox, LayoutBoxKey, LayoutTy, tree::LayoutBoxTree,
    },
    style::div::{DisplayInner, DisplayOuter},
    ui::{Node, NodeKey, Ui},
};

pub struct LayoutTreeBuilderCx {
    parents: Vec<LayoutBoxKey>,
    inline: Option<(InlineBox, InlineKey)>,
}

impl Default for LayoutTreeBuilderCx {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutTreeBuilderCx {
    pub fn new() -> Self {
        Self {
            parents: vec![],
            inline: None,
        }
    }

    pub fn builder<'a>(
        &'a mut self,
        ui: &'a Ui,
        tree: &'a mut LayoutBoxTree,
    ) -> LayoutTreeBuilder<'a> {
        LayoutTreeBuilder { cx: self, ui, tree }
    }
}

pub struct LayoutTreeBuilder<'a> {
    cx: &'a mut LayoutTreeBuilderCx,
    ui: &'a Ui,
    tree: &'a mut LayoutBoxTree,
}

impl LayoutTreeBuilder<'_> {
    /// Build a node into layout boxes and append inside box_id
    pub fn build(&mut self, id: NodeKey, box_id: LayoutBoxKey) {
        self.cx.parents.push(box_id);
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
                    let layout_box_id = self.add_child(LayoutBox::new(Some(id), LayoutTy::Block));
                    self.cx.parents.push(layout_box_id);
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
                let layout_box_id = self
                    .tree
                    .boxes
                    .insert(LayoutBox::new(None, LayoutTy::Block));
                self.cx.parents.push(layout_box_id);

                self.build_siblings(self.ui.first_child(id), true);

                self.commit_inlines();
                self.cx.inline = last_inline;
                let box_key = self.cx.parents.pop().unwrap();
                self.push_inline(InlineIns::Box(box_key));
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
                let mut inline_box = InlineBox::new();
                inline_box.inline_start = Some(key);
                self.cx.inline = Some((inline_box, key));
            }
        }
    }

    fn commit_inlines(&mut self) {
        let Some((inline_box, _)) = self.cx.inline.take() else {
            return;
        };
        let inline_key = self.tree.inline_boxes.insert(inline_box);
        let key = self
            .tree
            .boxes
            .insert(LayoutBox::new(None, LayoutTy::Inline(inline_key)));
        self.add_child_id(key);
    }

    fn add_child(&mut self, node: LayoutBox) -> LayoutBoxKey {
        let id = self.tree.boxes.insert(node);
        self.add_child_id(id);
        id
    }

    fn add_child_id(&mut self, id: LayoutBoxKey) {
        let Some(parent) = self.cx.parents.last().copied() else {
            return;
        };

        let Some(parent_node) = self.tree.boxes.get_mut(parent) else {
            return;
        };

        match parent_node.ty {
            LayoutTy::Block => {
                self.tree.boxes.append(parent, id);
            }

            LayoutTy::Inline(inline_box_key) => {
                let Some(item_start) = self
                    .tree
                    .inline_boxes
                    .get(inline_box_key)
                    .and_then(|node| node.inline_start)
                else {
                    return;
                };

                let inline_id = self.tree.inlines.insert(InlineIns::Box(id));
                self.tree.inlines.after(item_start, inline_id);
            }
        }
    }
}
