use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};

use crate::{
    style::{DisplayInner, DisplayOuter},
    tree::{LayoutItem, LayoutItemKey, LayoutItemTy, LayoutTree},
};

/// Builds layout items whose types describe their children's flow, retaining
/// compatible items during updates. Inline-block wrappers are anonymous blocks; their
/// bodies carry the source node and select their own children's flow.
/// Text nodes are inline-flow leaves; their properties and children are ignored.
/// A text leaf directly in block flow gets an anonymous inline container.
/// Text stays in the UI. Inline ancestry stays in the tree, including cloned
/// inline fragments around blocks. No instruction streams or text caches exist.
/// This represents layout objects, not laid-out fragments; see Chromium's
/// [LayoutNG overview](https://developer.chrome.com/docs/chromium/layoutng).
#[derive(Default)]
pub struct LayoutTreeBuilder {
    pending: Vec<Task>,
    frames: Vec<Frame>,
    flows: Vec<Flow>,
    spans: Vec<NodeKey>,
    scan: Vec<NodeKey>,
}

impl LayoutTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update a complete flow boundary, leaving unrelated items untouched.
    /// Use the returned key: incompatible roots are replaced in their old position.
    /// A deleted source deletes the target. Inline-block bodies are supported too.
    pub fn build(
        &mut self,
        ui: &Ui,
        root: NodeKey,
        tree: &mut LayoutTree,
        target: LayoutItemKey,
    ) -> Option<LayoutItemKey> {
        self.update(ui, root, tree, Some(target))
    }

    pub fn build_new(
        &mut self,
        ui: &Ui,
        root: NodeKey,
        tree: &mut LayoutTree,
    ) -> Option<LayoutItemKey> {
        self.update(ui, root, tree, None)
    }

    fn update(
        &mut self,
        ui: &Ui,
        root: NodeKey,
        tree: &mut LayoutTree,
        target: Option<LayoutItemKey>,
    ) -> Option<LayoutItemKey> {
        self.clear();

        if ui.node(root).is_none() {
            if let Some(key) = target {
                tree.delete_item(key);
            }

            return None;
        }

        if ui.node(root).is_some_and(|node| *node == Node::Text) {
            return self.update_text(tree, root, target);
        }

        let (outer, inner) = display(ui, root);
        let body = target.is_some_and(|key| {
            tree.items
                .get(key)
                .is_some_and(|item| item.ty != LayoutItemTy::Text && item.node == Some(root))
                && tree
                    .items
                    .parent(key)
                    .and_then(|key| tree.items.get(key))
                    .is_some_and(|item| item.ty == LayoutItemTy::Block && item.node.is_none())
        }) && outer == DisplayOuter::Inline && inner != DisplayInner::Flow;

        let split = !body
            && outer == DisplayOuter::Inline
            && inner == DisplayInner::Flow
            && self.contains_block(ui, root);

        let desired = if !body && outer == DisplayOuter::Inline && inner != DisplayInner::Flow {
            item(None, LayoutItemTy::Block)
        } else if split {
            item(None, LayoutItemTy::Block)
        } else {
            item(
                Some(root),
                if body || outer == DisplayOuter::Block {
                    self.content_type(ui, root)
                } else {
                    outer_type(outer)
                },
            )
        };

        let key = match target {
            Some(key) if tree.items.get(key).is_some_and(|old| compatible(*old, desired)) => {
                tree.items[key] = desired;

                key
            }
            _ => {
                let key = tree.add_item(desired);

                if let Some(old) = target {
                    if tree.items.get(old).is_some() {
                        tree.items.before(old, key);
                    }

                    tree.delete_item(old);
                }

                key
            }
        };

        self.push_frame(tree, key);
        self.push_flow();

        if split {
            self.pending.push(Task::Visit(root));
        } else if !body && outer == DisplayOuter::Inline && inner != DisplayInner::Flow {
            if self.start_body(ui, tree, root).is_none() {
                self.clear();
                tree.delete_item(key);

                return None;
            }

            self.pending.push(Task::Content(root));
        } else {
            self.pending.push(Task::Content(root));
        }

        let result = self
            .traverse(ui, tree)
            .and_then(|()| self.finish_flow(tree));
        self.clear();

        if result.is_some() {
            Some(key)
        } else {
            tree.delete_item(key);

            None
        }
    }

    fn traverse(&mut self, ui: &Ui, tree: &mut LayoutTree) -> Option<()> {
        while let Some(task) = self.pending.pop() {
            match task {
                Task::Visit(node) => {
                    if *ui.node(node)? == Node::Text {
                        self.resume_spans(tree)?;
                        self.text_child(tree, node)?;

                        continue;
                    }

                    let (outer, inner) = display(ui, node);

                    match outer {
                        DisplayOuter::Block => {
                            // Finish the leading fragments, but keep their source
                            // ancestry so it can resume after this block group.
                            let base = self.flows.last()?.frame;
                            while self.frames.len() > base + 1 {
                                self.finish_frame(tree)?;
                            }

                            let ty = self.content_type(ui, node);
                            let key = self.child(tree, item(Some(node), ty))?;
                            self.push_frame(tree, key);
                            self.push_flow();
                            self.pending.push(Task::EndFlow);
                        }

                        DisplayOuter::Inline => {
                            self.resume_spans(tree)?;
                            let source = (inner == DisplayInner::Flow).then_some(node);
                            let ty = if source.is_some() {
                                LayoutItemTy::Inline
                            } else {
                                LayoutItemTy::Block
                            };

                            let key = self.child_with_body(
                                tree,
                                item(source, ty),
                                source.is_none().then_some(node),
                            )?;
                            self.push_frame(tree, key);

                            if inner == DisplayInner::Flow {
                                self.spans.push(node);
                                self.pending.push(Task::EndInline);
                            } else {
                                self.pending.push(Task::EndFrame);
                                self.start_body(ui, tree, node)?;
                            }
                        }
                    }

                    self.pending.push(Task::Content(node));
                }

                Task::Content(node) => {
                    if display(ui, node).1 != DisplayInner::Content {
                        self.pending
                            .extend(ui.children(node).rev().map(Task::Visit));
                    }
                }

                Task::EndInline => {
                    // Retain empty trailing fragments as well as nonempty ones.
                    self.resume_spans(tree)?;
                    self.finish_frame(tree)?;
                    self.spans.pop()?;
                }

                Task::EndFlow => self.finish_flow(tree)?,
                Task::EndFrame => self.finish_frame(tree)?,
            }
        }

        Some(())
    }

    fn start_body(&mut self, ui: &Ui, tree: &mut LayoutTree, node: NodeKey) -> Option<()> {
        let ty = self.content_type(ui, node);
        let key = self.child(tree, item(Some(node), ty))?;
        self.push_frame(tree, key);
        self.push_flow();
        self.pending.push(Task::EndFlow);

        Some(())
    }

    fn update_text(
        &mut self,
        tree: &mut LayoutTree,
        node: NodeKey,
        target: Option<LayoutItemKey>,
    ) -> Option<LayoutItemKey> {
        if let Some(parent) = target.and_then(|key| tree.items.parent(key)) {
            // Start at the target and prune only up to its original next sibling.
            self.frames.push(Frame { key: parent, next: target });
            let next = target.and_then(|key| tree.items.next_sibling(key));
            let key = self.text_child(tree, node)?;
            let mut obsolete = self.frames.pop()?.next;

            while obsolete != next {
                let old = obsolete?;
                obsolete = tree.items.next_sibling(old);
                tree.delete_item(old);
            }

            return Some(key);
        }

        let desired = item(Some(node), LayoutItemTy::Text);
        let key = match target {
            Some(key) if tree.items.get(key) == Some(&desired) => key,
            _ => {
                let key = tree.add_item(desired);

                if let Some(old) = target {
                    tree.items.before(old, key);
                    tree.delete_item(old);
                }

                key
            }
        };
        delete_remaining(tree, tree.items.first_child(key));

        Some(key)
    }

    fn text_child(&mut self, tree: &mut LayoutTree, node: NodeKey) -> Option<LayoutItemKey> {
        let frame = self.frames.last_mut()?;
        let wrap = tree.items.get(frame.key)?.ty == LayoutItemTy::Block;
        let desired = item(Some(node), LayoutItemTy::Text);
        let anonymous = item(None, LayoutItemTy::Inline);
        let mut candidate = frame.next;

        // Move the existing leaf when the parent's flow changes.
        while let Some(key) = candidate {
            if wrap && tree.items.get(key) == Some(&desired) {
                let wrapper = tree.add_item(anonymous);
                tree.items.before(key, wrapper);
                tree.items.append(wrapper, key);

                if frame.next == Some(key) {
                    frame.next = Some(wrapper);
                }

                break;
            }

            if !wrap && tree.items.get(key) == Some(&anonymous) {
                if let Some(leaf) = tree.items.first_child(key) {
                    if tree.items.get(leaf) == Some(&desired) {
                        tree.items.before(key, leaf);

                        if frame.next == Some(key) {
                            frame.next = Some(leaf);
                        }

                        tree.delete_item(key);

                        break;
                    }
                }
            }

            candidate = tree.items.next_sibling(key);
        }

        let wrapper = if wrap {
            let key = self.child_with_body(tree, anonymous, Some(node))?;
            self.push_frame(tree, key);

            Some(key)
        } else {
            None
        };

        let leaf = self.child(tree, desired)?;
        delete_remaining(tree, tree.items.first_child(leaf));

        if wrapper.is_some() {
            self.finish_frame(tree)?;
        }

        Some(wrapper.unwrap_or(leaf))
    }

    fn resume_spans(&mut self, tree: &mut LayoutTree) -> Option<()> {
        let flow = *self.flows.last()?;
        let mut index = flow.span_start + self.frames.len().checked_sub(flow.frame + 1)?;

        while let Some(&node) = self.spans.get(index) {
            let key = self.child(tree, item(Some(node), LayoutItemTy::Inline))?;
            self.push_frame(tree, key);
            index += 1;
        }

        Some(())
    }

    fn child(&mut self, tree: &mut LayoutTree, desired: LayoutItem) -> Option<LayoutItemKey> {
        self.child_with_body(tree, desired, None)
    }

    fn child_with_body(
        &mut self,
        tree: &mut LayoutTree,
        desired: LayoutItem,
        body: Option<NodeKey>,
    ) -> Option<LayoutItemKey> {
        let frame = self.frames.last_mut()?;
        let mut candidate = frame.next;

        while let Some(key) = candidate {
            let matching_body = body.is_none_or(|node| {
                tree.items
                    .first_child(key)
                    .and_then(|child| tree.items.get(child))
                    .is_some_and(|item| item.node == Some(node))
            });

            if matching_body && tree.items.get(key).is_some_and(|old| compatible(*old, desired)) {
                tree.items[key] = desired;

                if frame.next == Some(key) {
                    frame.next = tree.items.next_sibling(key);
                } else if let Some(next) = frame.next {
                    tree.items.before(next, key);
                }

                return Some(key);
            }

            candidate = tree.items.next_sibling(key);
        }

        let key = tree.add_item(desired);
        if let Some(next) = frame.next {
            tree.items.before(next, key);
        } else {
            tree.items.append(frame.key, key);
        }

        Some(key)
    }

    fn push_frame(&mut self, tree: &LayoutTree, key: LayoutItemKey) {
        self.frames.push(Frame {
            key,
            next: tree.items.first_child(key),
        });
    }

    fn finish_frame(&mut self, tree: &mut LayoutTree) -> Option<()> {
        delete_remaining(tree, self.frames.pop()?.next);

        Some(())
    }

    fn push_flow(&mut self) {
        if let Some(frame) = self.frames.len().checked_sub(1) {
            self.flows.push(Flow {
                frame,
                span_start: self.spans.len(),
            });
        }
    }

    fn finish_flow(&mut self, tree: &mut LayoutTree) -> Option<()> {
        let flow = self.flows.pop()?;

        while self.frames.len() > flow.frame {
            self.finish_frame(tree)?;
        }

        self.spans.truncate(flow.span_start);

        Some(())
    }

    fn contains_block(&mut self, ui: &Ui, root: NodeKey) -> bool {
        self.scan.extend(ui.children(root));

        while let Some(node) = self.scan.pop() {
            if ui.node(node).is_some_and(|node| *node == Node::Text) {
                continue;
            }

            let (outer, inner) = display(ui, node);

            if outer == DisplayOuter::Block {
                self.scan.clear();

                return true;
            }

            if inner == DisplayInner::Flow {
                self.scan.extend(ui.children(node));
            }
        }

        false
    }

    fn content_type(&mut self, ui: &Ui, node: NodeKey) -> LayoutItemTy {
        let has_children = display(ui, node).1 != DisplayInner::Content
            && ui.children(node).next().is_some();

        if has_children && self.contains_block(ui, node) {
            LayoutItemTy::Block
        } else if has_children {
            LayoutItemTy::Inline
        } else {
            LayoutItemTy::Block
        }
    }

    fn clear(&mut self) {
        self.pending.clear();
        self.frames.clear();
        self.flows.clear();
        self.spans.clear();
        self.scan.clear();
    }
}

enum Task {
    Visit(NodeKey),
    Content(NodeKey),
    EndInline,
    EndFlow,
    EndFrame,
}

struct Frame {
    key: LayoutItemKey,
    next: Option<LayoutItemKey>,
}

#[derive(Clone, Copy)]
struct Flow {
    frame: usize,
    span_start: usize,
}

fn item(node: Option<NodeKey>, ty: LayoutItemTy) -> LayoutItem {
    LayoutItem { node, ty }
}

fn compatible(old: LayoutItem, desired: LayoutItem) -> bool {
    old == desired
        || (old.node.is_some()
            && old.node == desired.node
            && old.ty != LayoutItemTy::Text
            && desired.ty != LayoutItemTy::Text)
}

fn outer_type(outer: DisplayOuter) -> LayoutItemTy {
    match outer {
        DisplayOuter::Block => LayoutItemTy::Block,
        DisplayOuter::Inline => LayoutItemTy::Inline,
    }
}

fn display(ui: &Ui, node: NodeKey) -> (DisplayOuter, DisplayInner) {
    if ui.node(node).is_some_and(|node| *node == Node::Text) {
        return (DisplayOuter::Inline, DisplayInner::Flow);
    }

    (
        ui.prop::<DisplayOuter>(node)
            .map(|v| *v)
            .unwrap_or_default(),
        ui.prop::<DisplayInner>(node)
            .map(|v| *v)
            .unwrap_or_default(),
    )
}

fn delete_remaining(tree: &mut LayoutTree, mut next: Option<LayoutItemKey>) {
    while let Some(key) = next {
        next = tree.items.next_sibling(key);
        tree.delete_item(key);
    }
}
