use slotmap::{SlotMap, new_key_type};

use crate::node::{DisplayInner, DisplayOuter, Node, NodeKey, UiTree};

new_key_type! { pub struct BoxKey; }

#[derive(Debug)]
pub enum TreeNode {
    Box(BoxItem),
    Inline(InlineBoxItem),
}

#[derive(Debug)]
pub struct BoxItem {
    pub span: Option<NodeKey>,
    pub children: Vec<BoxKey>,
}

impl BoxItem {
    pub fn new(span: Option<NodeKey>) -> Self {
        Self {
            span,
            children: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct InlineBoxItem {
    pub children: Vec<InlineItem>,
}

impl InlineBoxItem {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum InlineItem {
    Node(NodeKey),
    Box(BoxKey),
}

pub struct BoxLayoutTree {
    pub map: SlotMap<BoxKey, TreeNode>,
    pub blocks: Vec<()>,
    pub inlines: Vec<()>,
}

impl BoxLayoutTree {
    pub fn new() -> Self {
        Self {
            map: SlotMap::with_key(),
            blocks: Vec::new(),
            inlines: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

pub struct BoxLayoutTreeCx {
    parents: Vec<BoxKey>,
    current_inline_box: Option<BoxKey>,
}

impl BoxLayoutTreeCx {
    pub fn new() -> Self {
        Self {
            parents: Vec::new(),
            current_inline_box: None,
        }
    }

    fn push_child(&mut self, tree: &mut BoxLayoutTree, id: BoxKey) {
        if let Some(parent) = self.parents.last().copied()
            && let Some(TreeNode::Box(block)) = tree.map.get_mut(parent)
        {
            block.children.push(id);
        }
    }

    /// Create a parent box
    fn create_parent_box(&mut self, tree: &mut BoxLayoutTree, span: Option<NodeKey>) -> BoxKey {
        let id = tree.map.insert(TreeNode::Box(BoxItem::new(span)));
        self.push_child(tree, id);
        self.parents.push(id);
        self.current_inline_box.take();
        id
    }

    /// Establish a new box context
    fn create_cx(&mut self, tree: &mut BoxLayoutTree, span: Option<NodeKey>) {
        let cx_box = tree.map.insert(TreeNode::Box(BoxItem::new(span)));
        self.with_inline_box(tree, |inline| {
            inline.children.push(InlineItem::Box(cx_box));
        });

        self.parents.push(cx_box);
    }

    fn with_inline_box<R>(
        &mut self,
        tree: &mut BoxLayoutTree,
        f: impl FnOnce(&mut InlineBoxItem) -> R,
    ) -> R {
        if let Some(inline_box) = self.current_inline_box
            && let Some(TreeNode::Inline(inline)) = tree.map.get_mut(inline_box)
        {
            return f(inline);
        }

        let mut inline_box = InlineBoxItem::new();
        let ret = f(&mut inline_box);
        let id = tree.map.insert(TreeNode::Inline(inline_box));
        self.push_child(tree, id);
        self.current_inline_box = Some(id);

        ret
    }

    pub fn build(&mut self, ui: &UiTree, root: NodeKey, tree: &mut BoxLayoutTree) -> BoxKey {
        tree.clear();

        let root_id = self.create_parent_box(tree, None);
        self.build_inner(ui, root, tree);

        self.parents.clear();

        root_id
    }

    fn build_inner(&mut self, ui: &UiTree, id: NodeKey, tree: &mut BoxLayoutTree) {
        let Some(node) = ui.get(id) else {
            return;
        };

        match node {
            Node::Div(div) => {
                let mut span = Some(id);
                let (display_outer, display_inner) = div.display.unwrap_or_default();

                let needs_block = display_outer == DisplayOuter::Block;
                if needs_block {
                    self.create_parent_box(tree, span.take());
                }

                let needs_new_cx = display_inner != DisplayInner::Flow;
                if needs_new_cx {
                    self.create_cx(tree, span.take());
                }

                for &child in ui.children(id) {
                    self.build_inner(ui, child, tree);
                }

                if needs_new_cx {
                    self.parents.pop();
                }

                if needs_block {
                    self.parents.pop();
                }
            }

            Node::Text(_) => {
                self.with_inline_box(tree, |inline| {
                    inline.children.push(InlineItem::Node(id));
                });
            }
        }
    }
}
