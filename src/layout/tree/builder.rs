use crate::{
    layout::{
        InlineBox, InlineBoxKey, InlineItem, InlineKey, LayoutBox, LayoutBoxKey, LayoutTy,
        tree::LayoutBoxTree,
    },
    style::div::{DisplayInner, DisplayOuter},
    ui::{Node, NodeKey, Ui},
};

pub struct LayoutTreeBuilderCx {
    parents: Vec<LayoutBoxKey>,
    inline_cx: Vec<InlineBoxCx>,
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
            inline_cx: vec![],
        }
    }

    pub fn builder<'a>(
        &'a mut self,
        ui: &'a Ui,
        tree: &'a mut LayoutBoxTree,
    ) -> LayoutTreeBuilder<'a> {
        LayoutTreeBuilder {
            inner: self,
            ui,
            tree,
        }
    }
}

pub struct LayoutTreeBuilder<'a> {
    inner: &'a mut LayoutTreeBuilderCx,
    ui: &'a Ui,
    tree: &'a mut LayoutBoxTree,
}

impl LayoutTreeBuilder<'_> {
    /// Rebuild children nodes inside box_id
    pub fn build_children(&mut self, id: NodeKey, box_id: LayoutBoxKey) {
        let mut next_child = self.tree.boxes.first_child(box_id);
        while let Some(child) = next_child {
            next_child = self.tree.boxes.next_sibling(child);
            self.tree.boxes.delete_node(child);
        }

        self.inner.parents.push(box_id);
        self.inner.inline_cx.push(InlineBoxCx::new());
        self.build_inner(id);
        // commit remaining inline box
        self.commit_inline_box();
        self.inner.parents.clear();
        self.inner.inline_cx.clear();
    }

    fn build_inner(&mut self, id: NodeKey) {
        let Some(node) = self.ui.node(id) else {
            return;
        };

        match *node {
            Node::Div => {
                let display_outer = self
                    .ui
                    .prop::<DisplayOuter>(id)
                    .as_deref()
                    .cloned()
                    .unwrap_or_default();

                match display_outer {
                    DisplayOuter::Block => {
                        self.commit_inline_box();
                        let id = self.add_child(LayoutBox::new(Some(id), LayoutTy::Block));
                        self.inner.parents.push(id);
                    }
                    DisplayOuter::Inline => {
                        self.inner.inline_cx.last_mut().unwrap().push_span(id);
                    }
                }

                let display_inner = self
                    .ui
                    .prop::<DisplayInner>(id)
                    .as_deref()
                    .cloned()
                    .unwrap_or_default();
                let needs_new_cx = display_inner != DisplayInner::Flow;
                if needs_new_cx {
                    let id = self
                        .tree
                        .boxes
                        .insert(LayoutBox::new(None, LayoutTy::Block));
                    self.inner.parents.push(id);
                    self.inner
                        .inline_cx
                        .last_mut()
                        .unwrap()
                        .push_item(self.tree, InlineItem::Box(id));
                    self.inner.inline_cx.push(InlineBoxCx::new());
                }

                for child in self.ui.cursor(self.ui.first_child(id)) {
                    self.build_inner(child);
                }

                if needs_new_cx {
                    if let Some((span, inline_box_id)) =
                        self.inner.inline_cx.pop().unwrap().finish(self.tree)
                    {
                        let id = self
                            .tree
                            .boxes
                            .insert(LayoutBox::new(span, LayoutTy::Inline(inline_box_id)));
                        self.add_child_id(id);
                    }
                    self.inner.parents.pop();
                }

                match display_outer {
                    DisplayOuter::Block => {
                        self.commit_inline_box();
                        self.inner.parents.pop();
                    }
                    DisplayOuter::Inline => {
                        self.inner.inline_cx.last_mut().unwrap().pop_span();
                    }
                }
            }

            Node::Text(_) => {
                self.inner
                    .inline_cx
                    .last_mut()
                    .unwrap()
                    .push_item(self.tree, InlineItem::Text(id));
            }
        }
    }

    fn commit_inline_box(&mut self) {
        if let Some((span, inline_box_id)) =
            self.inner.inline_cx.last_mut().unwrap().finish(self.tree)
        {
            let id = self
                .tree
                .boxes
                .insert(LayoutBox::new(span, LayoutTy::Inline(inline_box_id)));
            self.add_child_id(id);
        }
    }

    fn add_child(&mut self, node: LayoutBox) -> LayoutBoxKey {
        let id = self.tree.boxes.insert(node);
        self.add_child_id(id);
        id
    }

    fn add_child_id(&mut self, id: LayoutBoxKey) {
        let Some(parent) = self.inner.parents.last().copied() else {
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
                    .and_then(|node| node.item_start)
                else {
                    return;
                };

                let inline_id = self.tree.inlines.insert(InlineItem::Box(id));
                self.tree.inlines.after(item_start, inline_id);
            }
        }
    }
}

#[derive(Debug)]
struct InlineBoxCx {
    span_stack: Vec<NodeKey>,
    first_key: Option<InlineKey>,
    last_key: Option<InlineKey>,
}

impl InlineBoxCx {
    pub fn new() -> Self {
        Self {
            span_stack: Vec::new(),
            first_key: None,
            last_key: None,
        }
    }

    pub fn push_span(&mut self, key: NodeKey) {
        self.span_stack.push(key);
    }

    pub fn pop_span(&mut self) -> Option<NodeKey> {
        self.span_stack.pop()
    }

    pub fn push_item(&mut self, tree: &mut LayoutBoxTree, item: InlineItem) {
        let span = self.span_stack.last().copied();

        let id = if let Some(span) = span {
            let mut inline_box = InlineBox::new();
            let inline_id = tree.inlines.insert(item);
            inline_box.item_start = Some(inline_id);
            let inner_box_id = tree.inline_boxes.insert(inline_box);

            let id = tree
                .boxes
                .insert(LayoutBox::new(Some(span), LayoutTy::Inline(inner_box_id)));

            tree.inlines.insert(InlineItem::Box(id))
        } else {
            tree.inlines.insert(item)
        };

        if self.first_key.is_none() {
            self.first_key = Some(id);
        }

        if let Some(prev_last_id) = self.last_key.replace(id) {
            tree.inlines.after(prev_last_id, id);
        }
    }

    pub fn finish(&mut self, tree: &mut LayoutBoxTree) -> Option<(Option<NodeKey>, InlineBoxKey)> {
        let span = self.span_stack.last().copied();
        let first_key = self.first_key?;
        self.first_key = None;
        self.last_key = None;

        let mut inline_box = InlineBox::new();
        inline_box.item_start = Some(first_key);
        Some((span, tree.inline_boxes.insert(inline_box)))
    }
}
