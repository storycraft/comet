use crate::{
    layout::{
        InlineBox, InlineIns, InlineKey, LayoutBox, LayoutBoxKey, LayoutTy, tree::LayoutBoxTree,
    },
    style::div::{DisplayInner, DisplayOuter},
    ui::{Node, NodeKey, Ui},
};

pub struct LayoutTreeBuilderCx {
    parents: Vec<LayoutBoxKey>,
    inline: Option<(InlineKey, InlineKey)>,
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
        self.build_inner(id);
        // commit remaining inlines
        self.commit_inlines();
        self.cx.parents.clear();
    }

    fn build_inner(&mut self, id: NodeKey) {
        let Some(node) = self.ui.node(id) else {
            return;
        };
        match *node {
            Node::Div => {
                self.build_div(id);
            }
            Node::Text(_) => {
                self.build_text(id);
            }
        }
    }

    fn build_text(&mut self, id: NodeKey) {
        self.push_inline(InlineIns::Text(id));
    }

    fn build_div(&mut self, id: NodeKey) {
        let display_outer = self
            .ui
            .prop::<DisplayOuter>(id)
            .as_deref()
            .cloned()
            .unwrap_or_default();

        match display_outer {
            DisplayOuter::Block => {
                self.commit_inlines();
                let id = self.add_child(LayoutBox::new(Some(id), LayoutTy::Block));
                self.cx.parents.push(id);
            }
            DisplayOuter::Inline => {
                self.push_inline(InlineIns::PushInlineBox(id));
            }
        }

        let display_inner = self
            .ui
            .prop::<DisplayInner>(id)
            .as_deref()
            .cloned()
            .unwrap_or_default();
        let needs_new_cx = display_inner != DisplayInner::Flow;
        let last_inline = self.cx.inline;
        if needs_new_cx {
            self.cx.inline.take();
            let id = self
                .tree
                .boxes
                .insert(LayoutBox::new(None, LayoutTy::Block));
            self.cx.parents.push(id);
        }

        for child in self.ui.cursor(self.ui.first_child(id)) {
            self.build_inner(child);
        }

        if needs_new_cx {
            self.commit_inlines();
            self.cx.inline = last_inline;
            let box_key = self.cx.parents.pop().unwrap();
            self.push_inline(InlineIns::Box(box_key));
        }

        match display_outer {
            DisplayOuter::Block => {
                self.commit_inlines();
                self.cx.parents.pop();
            }
            DisplayOuter::Inline => {
                self.push_inline(InlineIns::PopInlineBox);
            }
        }
    }

    fn push_inline(&mut self, ins: InlineIns) {
        let key = self.tree.inlines.insert(ins);
        match self.cx.inline {
            Some((first, last)) => {
                self.tree.inlines.after(last, key);
                self.cx.inline = Some((first, key));
            },
            None => {
                self.cx.inline = Some((key, key));
            },
        }
    }

    fn commit_inlines(&mut self) {
        let Some((first, _)) = self.cx.inline.take() else {
            return;
        };
        let inline_key = self.tree.inline_boxes.insert(InlineBox {
            inline_start: Some(first),
            ..Default::default()
        });
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
