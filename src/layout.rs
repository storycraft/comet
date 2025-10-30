pub mod inline;
mod taffy_impl;

use slotmap::{Key, SlotMap, new_key_type};
use taffy::compute_root_layout;

use crate::{
    layout::taffy_impl::{TaffyLayoutImpl, to_taffy_key},
    node::{DisplayInner, DisplayOuter, Node, NodeKey, UiTree},
};

new_key_type! { pub struct BoxKey; }

#[derive(Debug)]
pub struct BoxNode {
    pub span: Option<NodeKey>,

    pub prev_sibiling: Option<BoxKey>,
    pub next_sibiling: Option<BoxKey>,

    pub cache: taffy::Cache,
    pub unrounded_layout: taffy::Layout,
    pub layout: taffy::Layout,

    pub ty: BoxNodeTy,
}

impl BoxNode {
    pub fn new(span: Option<NodeKey>, ty: BoxNodeTy) -> Self {
        Self {
            span,

            prev_sibiling: None,
            next_sibiling: None,

            cache: taffy::Cache::new(),
            unrounded_layout: taffy::Layout::new(),
            layout: taffy::Layout::new(),

            ty,
        }
    }
}

#[derive(Debug)]
pub enum BoxNodeTy {
    Block(BlockBox),
    Inline(InlineBox),
}

#[derive(Debug)]
pub struct BlockBox {
    pub first_child: Option<BoxKey>,
    pub last_child: Option<BoxKey>,
    pub children_count: usize,
}

impl BlockBox {
    pub fn new() -> Self {
        Self {
            first_child: None,
            last_child: None,
            children_count: 0,
        }
    }
}

impl Default for BlockBox {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct InlineBox {
    pub children: Vec<InlineItem>,
}

impl Default for InlineBox {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineBox {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum InlineItem {
    Text { start: usize, end: usize },
    Box(BoxKey),
}

pub struct BoxLayoutTree {
    pub root: BoxKey,
    pub map: SlotMap<BoxKey, BoxNode>,
    pub texts: String,
}

impl Default for BoxLayoutTree {
    fn default() -> Self {
        Self::new()
    }
}

impl BoxLayoutTree {
    pub fn new() -> Self {
        Self {
            root: BoxKey::null(),
            map: SlotMap::with_key(),
            texts: String::new(),
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }

    pub fn compute_layout(
        &mut self,
        ui: &mut UiTree,
        available_space: taffy::Size<taffy::AvailableSpace>,
    ) {
        let root = to_taffy_key(self.root);
        compute_root_layout(&mut TaffyLayoutImpl(self, ui), root, available_space);
    }
}

pub struct BoxLayoutTreeCx {
    parents: Vec<BoxKey>,
    inline_cx: Vec<InlineBoxCx>,
    inline_text_buf: String,
}

impl Default for BoxLayoutTreeCx {
    fn default() -> Self {
        Self::new()
    }
}

impl BoxLayoutTreeCx {
    pub fn new() -> Self {
        Self {
            parents: Vec::new(),
            inline_cx: Vec::new(),
            inline_text_buf: String::new(),
        }
    }

    fn commit_text(&mut self, tree: &mut BoxLayoutTree) {
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

    fn commit_inline_box(&mut self, tree: &mut BoxLayoutTree) {
        if let Some(id) = self.inline_cx.last_mut().unwrap().finish(tree) {
            self.add_child_id(tree, id);
        }
    }

    fn add_child(&mut self, tree: &mut BoxLayoutTree, node: BoxNode) -> BoxKey {
        let id = tree.map.insert(node);
        self.add_child_id(tree, id);
        id
    }

    fn add_child_id(&mut self, tree: &mut BoxLayoutTree, id: BoxKey) {
        let Some(parent) = self.parents.last().copied() else {
            return;
        };

        let Some(parent_node) = tree.map.get_mut(parent) else {
            return;
        };

        match parent_node.ty {
            BoxNodeTy::Block(ref mut item) => {
                item.children_count += 1;
                if item.first_child.is_none() {
                    item.first_child = Some(id);
                }

                let Some(last_child_id) = item.last_child.replace(id) else {
                    return;
                };

                if let Some(last_child_node) = tree.map.get_mut(last_child_id) {
                    last_child_node.next_sibiling = Some(id);
                }

                if let Some(node) = tree.map.get_mut(id) {
                    node.prev_sibiling = Some(last_child_id);
                }
            }
            BoxNodeTy::Inline(ref mut item) => {
                item.children.push(InlineItem::Box(id));
            }
        }
    }

    pub fn build(&mut self, ui: &UiTree, root: NodeKey, tree: &mut BoxLayoutTree) {
        tree.clear();

        let root_id = tree
            .map
            .insert(BoxNode::new(None, BoxNodeTy::Block(BlockBox::new())));
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

    fn build_inner(&mut self, ui: &UiTree, id: NodeKey, tree: &mut BoxLayoutTree) {
        let Some(node) = ui.get(id) else {
            return;
        };

        match node {
            Node::Div(div) => {
                self.commit_text(tree);

                let (display_outer, display_inner) = div.display.unwrap_or_default();

                match display_outer {
                    DisplayOuter::Block => {
                        self.commit_inline_box(tree);
                        let id = self.add_child(
                            tree,
                            BoxNode::new(Some(id), BoxNodeTy::Block(BlockBox::new())),
                        );
                        self.parents.push(id);
                    }
                    DisplayOuter::Inline => {
                        self.inline_cx.last_mut().unwrap().push_span(id);
                    }
                }

                let needs_new_cx = display_inner != DisplayInner::Flow;
                if needs_new_cx {
                    let id = tree
                        .map
                        .insert(BoxNode::new(None, BoxNodeTy::Block(BlockBox::new())));
                    self.parents.push(id);
                    self.inline_cx
                        .last_mut()
                        .unwrap()
                        .push_item(tree, InlineItem::Box(id));
                    self.inline_cx.push(InlineBoxCx::new());
                }

                for &child in ui.children(id) {
                    self.build_inner(ui, child, tree);
                }
                self.commit_text(tree);

                if needs_new_cx {
                    if let Some(id) = self.inline_cx.pop().unwrap().finish(tree) {
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

            Node::Text(text) => {
                self.inline_text_buf.push_str(text);
            }
        }
    }
}

#[derive(Debug)]
struct InlineBoxCx {
    span_stack: Vec<NodeKey>,
    items: Vec<InlineItem>,
}

impl InlineBoxCx {
    pub fn new() -> Self {
        Self {
            span_stack: Vec::new(),
            items: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.span_stack.clear();
        self.items.clear();
    }

    pub fn push_span(&mut self, key: NodeKey) {
        self.span_stack.push(key);
    }

    pub fn pop_span(&mut self) -> Option<NodeKey> {
        self.span_stack.pop()
    }

    pub fn push_item(&mut self, tree: &mut BoxLayoutTree, item: InlineItem) {
        let span = self.span_stack.last().copied();

        if let Some(span) = span {
            let mut inline_box = InlineBox::new();
            inline_box.children.push(item);
            let id = tree
                .map
                .insert(BoxNode::new(Some(span), BoxNodeTy::Inline(inline_box)));
            self.items.push(InlineItem::Box(id));
        } else {
            self.items.push(item);
        }
    }

    pub fn finish(&mut self, tree: &mut BoxLayoutTree) -> Option<BoxKey> {
        match self.items.len() {
            0 => return None,
            1 => {
                if let Some(&InlineItem::Box(id)) = self.items.last() {
                    self.items.clear();
                    return Some(id);
                }
            }
            _ => {}
        }

        let mut inline_box = InlineBox::new();
        inline_box.children.append(&mut self.items);
        Some(tree.map.insert(BoxNode::new(
            self.span_stack.last().copied(),
            BoxNodeTy::Inline(inline_box),
        )))
    }
}
