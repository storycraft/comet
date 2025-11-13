use slotmap::{Key, SlotMap};
use std::fmt::Debug;
use taffy::{AvailableSpace, Size};

use crate::{
    layout::{
        InlineBox, InlineBoxKey, InlineItem, InlineKey, LayoutBox, LayoutBoxKey, LayoutTy,
        taffy::TaffyLayoutImpl,
    },
    style::div::{DisplayInner, DisplayOuter},
    tree::SlotTree,
    ui::{Node, NodeKey, Ui},
};

pub struct LayoutBoxTree {
    pub root: LayoutBoxKey,
    pub boxes: SlotTree<LayoutBoxKey, LayoutBox>,
    pub inline_boxes: SlotMap<InlineBoxKey, InlineBox>,
    pub inlines: SlotTree<InlineKey, InlineItem>,
    pub texts: String,
}

impl LayoutBoxTree {
    pub fn new() -> Self {
        Self {
            root: LayoutBoxKey::null(),
            boxes: SlotTree::new(),
            inline_boxes: SlotMap::with_key(),
            inlines: SlotTree::new(),
            texts: String::new(),
        }
    }

    pub fn clear(&mut self) {
        self.boxes.clear();
    }

    pub fn compute_layout(&mut self, ui: &mut Ui, available_space: Size<AvailableSpace>) {
        let root = self.root;
        TaffyLayoutImpl::new(self, ui).compute_layout(root, available_space);
    }
}

impl Default for LayoutBoxTree {
    fn default() -> Self {
        Self::new()
    }
}

pub struct LayoutBoxTreeCx {
    parents: Vec<LayoutBoxKey>,
    inline_cx: Vec<InlineBoxCx>,
    inline_text_buf: String,
}

impl Default for LayoutBoxTreeCx {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutBoxTreeCx {
    pub fn new() -> Self {
        Self {
            parents: Vec::new(),
            inline_cx: Vec::new(),
            inline_text_buf: String::new(),
        }
    }

    fn commit_text(&mut self, tree: &mut LayoutBoxTree) {
        if self.inline_text_buf.is_empty() {
            return;
        }

        let start = tree.texts.len();
        let end = start + self.inline_text_buf.len();
        tree.texts.extend(self.inline_text_buf.drain(..));
        self.inline_cx
            .last_mut()
            .unwrap()
            .push_item(tree, InlineItem::Text { start, end });
    }

    fn commit_inline_box(&mut self, tree: &mut LayoutBoxTree) {
        if let Some((span, inline_box_id)) = self.inline_cx.last_mut().unwrap().finish(tree) {
            let id = tree
                .boxes
                .insert(LayoutBox::new(span, LayoutTy::Inline(inline_box_id)));
            self.add_child_id(tree, id);
        }
    }

    fn add_child(&mut self, tree: &mut LayoutBoxTree, node: LayoutBox) -> LayoutBoxKey {
        let id = tree.boxes.insert(node);
        self.add_child_id(tree, id);
        id
    }

    fn add_child_id(&mut self, tree: &mut LayoutBoxTree, id: LayoutBoxKey) {
        let Some(parent) = self.parents.last().copied() else {
            return;
        };

        let Some(parent_node) = tree.boxes.get_mut(parent) else {
            return;
        };

        match parent_node.ty {
            LayoutTy::Block => {
                tree.boxes.append(parent, id);
            }

            LayoutTy::Inline(inline_box_key) => {
                let Some(item_start) = tree
                    .inline_boxes
                    .get(inline_box_key)
                    .and_then(|node| node.item_start)
                else {
                    return;
                };

                let inline_id = tree.inlines.insert(InlineItem::Box(id));
                tree.inlines.after(item_start, inline_id);
            }
        }
    }

    pub fn build(&mut self, ui: &Ui, root: NodeKey, tree: &mut LayoutBoxTree) {
        tree.clear();

        let root_id = tree.boxes.insert(LayoutBox::new(None, LayoutTy::Block));
        tree.root = root_id;
        self.parents.push(root_id);
        self.inline_cx.push(InlineBoxCx::new());
        self.build_inner(ui, root, tree);
        // commit remaining texts
        self.commit_text(tree);
        // commit remaining inline box
        self.commit_inline_box(tree);

        self.parents.clear();
        self.inline_cx.clear();
    }

    fn build_inner(&mut self, ui: &Ui, id: NodeKey, tree: &mut LayoutBoxTree) {
        let Some(node) = ui.node(id) else {
            return;
        };

        match *node {
            Node::Div => {
                self.commit_text(tree);

                let display_outer = ui
                    .prop::<&DisplayOuter>(id)
                    .as_deref()
                    .cloned()
                    .unwrap_or_default();
                match display_outer {
                    DisplayOuter::Block => {
                        self.commit_inline_box(tree);
                        let id = self.add_child(tree, LayoutBox::new(Some(id), LayoutTy::Block));
                        self.parents.push(id);
                    }
                    DisplayOuter::Inline => {
                        self.inline_cx.last_mut().unwrap().push_span(id);
                    }
                }

                let display_inner = ui
                    .prop::<&DisplayInner>(id)
                    .as_deref()
                    .cloned()
                    .unwrap_or_default();
                let needs_new_cx = display_inner != DisplayInner::Flow;
                if needs_new_cx {
                    let id = tree.boxes.insert(LayoutBox::new(None, LayoutTy::Block));
                    self.parents.push(id);
                    self.inline_cx
                        .last_mut()
                        .unwrap()
                        .push_item(tree, InlineItem::Box(id));
                    self.inline_cx.push(InlineBoxCx::new());
                }

                for child in ui.cursor(ui.first_child(id)) {
                    self.build_inner(ui, child, tree);
                }
                self.commit_text(tree);

                if needs_new_cx {
                    if let Some((span, inline_box_id)) = self.inline_cx.pop().unwrap().finish(tree)
                    {
                        let id = tree
                            .boxes
                            .insert(LayoutBox::new(span, LayoutTy::Inline(inline_box_id)));
                        self.add_child_id(tree, id);
                    }
                    self.parents.pop();
                }

                match display_outer {
                    DisplayOuter::Block => {
                        self.commit_inline_box(tree);
                        self.parents.pop();
                    }
                    DisplayOuter::Inline => {
                        self.inline_cx.last_mut().unwrap().pop_span();
                    }
                }
            }

            Node::Text(ref text) => {
                self.inline_text_buf.push_str(text);
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
