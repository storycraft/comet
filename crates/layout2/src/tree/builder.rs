use std::collections::VecDeque;

use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    style::bx::{DisplayInner, DisplayOuter},
    tree::{ChildLayoutMode, LayoutItem, LayoutItemKey, LayoutItemTy, LayoutTree},
};

#[derive(Default)]
pub struct LayoutTreeBuilder {
    pending: Vec<Task>,
    frames: Vec<Frame>,
    flows: Vec<Flow>,
    spans: Vec<NodeKey>,
    scan: Vec<NodeKey>,
    // Lazy indexes can retain moved items; never reuse an item twice per update.
    claimed: FxHashSet<LayoutItemKey>,
    last_items: FxHashMap<NodeKey, LayoutItemKey>,
}

impl LayoutTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a new subtree with `None`, or reconcile an existing one with `Some(key)`.
    pub fn build(
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
        let split = outer == DisplayOuter::Inline
            && inner == DisplayInner::Flow
            && self.contains_block(ui, root);

        let desired = if split {
            item(LayoutItemTy::AnonymousBlock, ChildLayoutMode::Block)
        } else {
            self.source_item(ui, root)
        };

        let key = update_item(tree, target, desired);
        self.record_item(tree, key);

        self.push_frame(tree, key);
        self.push_flow(tree);

        if split {
            self.pending.push(Task::Visit(root));
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

                    if outer == DisplayOuter::Block {
                        let base = self.flows.last()?.frame;

                        while self.frames.len() > base + 1 {
                            self.finish_frame(tree)?;
                        }

                        self.flows.last_mut()?.inline_run = false;
                    } else {
                        self.resume_spans(tree)?;
                    }

                    let desired = self.source_item(ui, node);
                    let key = self.child(tree, desired)?;
                    self.push_frame(tree, key);

                    if outer == DisplayOuter::Inline && inner == DisplayInner::Flow {
                        self.spans.push(node);
                        self.pending.push(Task::EndInline);
                    } else {
                        self.push_flow(tree);
                        self.pending.push(Task::EndFlow);
                    }

                    self.pending.push(Task::Content(node));
                }

                Task::Content(node) => {
                    if tree.items[self.frames.last()?.key].child_layout != ChildLayoutMode::None {
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
            }
        }

        Some(())
    }

    fn source_item(&mut self, ui: &Ui, node: NodeKey) -> LayoutItem {
        let (outer, inner) = display(ui, node);

        let ty = match (outer, inner) {
            (DisplayOuter::Block, _) => LayoutItemTy::Block(node),
            (DisplayOuter::Inline, DisplayInner::Flow) => LayoutItemTy::Inline(node),
            (DisplayOuter::Inline, _) => LayoutItemTy::AtomicInline(node),
        };

        let child_layout = if matches!(ty, LayoutItemTy::Inline(_)) {
            ChildLayoutMode::Inline
        } else {
            self.content_mode(ui, node)
        };

        item(ty, child_layout)
    }

    fn update_text(
        &mut self,
        tree: &mut LayoutTree,
        node: NodeKey,
        target: Option<LayoutItemKey>,
    ) -> Option<LayoutItemKey> {
        if let Some(parent) = target.and_then(|key| tree.items.parent(key)) {
            // Start at the target and prune only up to its original next sibling.
            self.frames.push(Frame {
                key: parent,
                next: target,
                index: None,
                adoption: None,
            });
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

        let desired = item(LayoutItemTy::Text(node), ChildLayoutMode::None);
        let key = update_item(tree, target, desired);
        self.record_item(tree, key);
        delete_remaining(tree, tree.items.first_child(key));

        Some(key)
    }

    fn text_child(&mut self, tree: &mut LayoutTree, node: NodeKey) -> Option<LayoutItemKey> {
        let frame = self.frames.last_mut()?;
        let wrap = tree.items.get(frame.key)?.child_layout == ChildLayoutMode::Block;
        let desired = item(LayoutItemTy::Text(node), ChildLayoutMode::None);
        let anonymous = item(LayoutItemTy::AnonymousBlock, ChildLayoutMode::Inline);

        // Whole-flow updates normalize runs in push_flow/resume_spans. Only a
        // standalone text update needs to repair its surrounding wrapper here.
        let mut candidate = self.flows.is_empty().then_some(frame.next).flatten();

        // Move the existing leaf when the parent's flow changes.
        while let Some(key) = candidate {
            if matches_item(
                tree,
                key,
                if wrap { anonymous } else { desired },
                wrap.then_some(node),
            ) {
                break;
            }

            if wrap && tree.items.get(key) == Some(&desired) {
                let wrapper = tree.add_item(anonymous);
                tree.items.before(key, wrapper);
                tree.items.append(wrapper, key);

                if frame.next == Some(key) {
                    frame.next = Some(wrapper);
                }

                break;
            }

            if !wrap
                && tree.items.get(key) == Some(&anonymous)
                && let Some(leaf) = tree.items.first_child(key)
                && tree.items.get(leaf) == Some(&desired)
            {
                tree.items.before(key, leaf);

                if frame.next == Some(key) {
                    frame.next = Some(leaf);
                }

                tree.delete_item(key);

                break;
            }

            candidate = tree.items.next_sibling(key);
        }

        let wrapper = if wrap {
            let key = self.child_with_text(tree, anonymous, Some(node))?;
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

        if self.frames.len() == flow.frame + 1
            && tree.items[self.frames[flow.frame].key].child_layout == ChildLayoutMode::Block
        {
            let key = self.child(
                tree,
                item(LayoutItemTy::AnonymousBlock, ChildLayoutMode::Inline),
            )?;
            self.push_frame(tree, key);
            self.flows.last_mut()?.inline_run = true;
        }

        let flow = *self.flows.last()?;
        let base = flow.frame + 1 + usize::from(flow.inline_run);
        let mut index = flow.span_start + self.frames.len().checked_sub(base)?;

        while let Some(&node) = self.spans.get(index) {
            let key = self.child(
                tree,
                item(LayoutItemTy::Inline(node), ChildLayoutMode::Inline),
            )?;
            self.push_frame(tree, key);
            index += 1;
        }

        Some(())
    }

    fn child(&mut self, tree: &mut LayoutTree, desired: LayoutItem) -> Option<LayoutItemKey> {
        self.child_with_text(tree, desired, None)
    }

    fn child_with_text(
        &mut self,
        tree: &mut LayoutTree,
        desired: LayoutItem,
        text: Option<NodeKey>,
    ) -> Option<LayoutItemKey> {
        let frame = self.frames.last_mut()?;

        if let Some(key) = frame.find(tree, desired, text, &self.claimed) {
            replace_item(tree, key, desired);

            if frame.next == Some(key) {
                frame.next = tree.items.next_sibling(key);
            } else {
                frame.insert(tree, key);
            }

            self.claimed.insert(key);
            self.record_item(tree, key);

            return Some(key);
        }

        // A new inline run can adopt items from the previous direct child list.
        if let Some(flow) = self.flows.last().copied()
            && flow.inline_run
            && self.frames.len() == flow.frame + 2
        {
            let parent = &mut self.frames[flow.frame];

            if let Some(key) = parent.adopt(tree, desired, text, &self.claimed) {
                if parent.next == Some(key) {
                    parent.next = tree.items.next_sibling(key);
                }

                replace_item(tree, key, desired);
                self.frames.last()?.insert(tree, key);
                self.claimed.insert(key);
                self.record_item(tree, key);

                return Some(key);
            }
        }

        let frame = self.frames.last_mut()?;
        let key = tree.add_item(desired);
        frame.insert(tree, key);
        self.claimed.insert(key);
        self.record_item(tree, key);

        Some(key)
    }

    fn record_item(&mut self, tree: &mut LayoutTree, key: LayoutItemKey) {
        let ty = tree.items[key].ty;

        if let Some(node) = ty.node() {
            if let Some(prev) = self.last_items.insert(node, key) {
                if matches!(ty, LayoutItemTy::Inline(_)) {
                    tree.items[prev].continuation.next = Some(key);
                    tree.items[key].continuation.prev = Some(prev);
                }
            } else {
                tree.map_first_item(node, key);
            }
        }
    }

    fn push_frame(&mut self, tree: &LayoutTree, key: LayoutItemKey) {
        self.frames.push(Frame {
            key,
            next: tree.items.first_child(key),
            index: None,
            adoption: None,
        });
    }

    fn finish_frame(&mut self, tree: &mut LayoutTree) -> Option<()> {
        let frame = self.frames.pop()?;

        if self
            .flows
            .last()
            .is_some_and(|flow| flow.inline_run && self.frames.len() == flow.frame + 1)
        {
            // Keep the unconsumed tail available when a block splits an old run.
            let parent = self.frames.last_mut()?;
            let mut next = frame.next;
            let first = next;
            let mut promoted = Vec::new();

            while let Some(key) = next {
                next = tree.items.next_sibling(key);

                parent.insert(tree, key);
                promoted.push(key);
            }

            // Promoted tails precede the parent's old unconsumed siblings.
            for key in promoted.into_iter().rev() {
                if let Some(index) = &mut parent.index {
                    index.entry(tree.items[key].ty).or_default().push_front(key);
                }

                if let Some(index) = &mut parent.adoption {
                    index.entry(tree.items[key].ty).or_default().push_front(key);
                }
            }

            if first.is_some() {
                parent.next = first;
            }
        } else {
            delete_remaining(tree, frame.next);
        }

        Some(())
    }

    fn push_flow(&mut self, tree: &mut LayoutTree) {
        if let Some(frame) = self.frames.len().checked_sub(1) {
            let key = self.frames[frame].key;

            // When block flow becomes inline flow, unwrap its old inline runs.
            if tree.items[key].child_layout == ChildLayoutMode::Inline {
                let mut next = tree.items.first_child(key);

                while let Some(child) = next {
                    next = tree.items.next_sibling(child);

                    if tree.items[child]
                        == item(LayoutItemTy::AnonymousBlock, ChildLayoutMode::Inline)
                    {
                        while let Some(content) = tree.items.first_child(child) {
                            tree.items.before(child, content);
                        }

                        tree.delete_item(child);
                    }
                }

                self.frames[frame].next = tree.items.first_child(key);
            }

            self.flows.push(Flow {
                frame,
                span_start: self.spans.len(),
                inline_run: false,
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

    fn content_mode(&mut self, ui: &Ui, node: NodeKey) -> ChildLayoutMode {
        if display(ui, node).1 == DisplayInner::Content {
            ChildLayoutMode::None
        } else if ui.children(node).next().is_none() || self.contains_block(ui, node) {
            ChildLayoutMode::Block
        } else {
            ChildLayoutMode::Inline
        }
    }

    fn clear(&mut self) {
        self.pending.clear();
        self.frames.clear();
        self.flows.clear();
        self.spans.clear();
        self.scan.clear();
        self.claimed.clear();
        self.last_items.clear();
    }
}

enum Task {
    Visit(NodeKey),
    Content(NodeKey),
    EndInline,
    EndFlow,
}

struct Frame {
    key: LayoutItemKey,
    next: Option<LayoutItemKey>,
    index: Option<Candidates>,
    adoption: Option<Candidates>,
}

// Inline sources can produce multiple fragments, so preserve candidate order.
type Candidates = FxHashMap<LayoutItemTy, VecDeque<LayoutItemKey>>;

impl Frame {
    fn find(
        &mut self,
        tree: &LayoutTree,
        desired: LayoutItem,
        text: Option<NodeKey>,
        claimed: &FxHashSet<LayoutItemKey>,
    ) -> Option<LayoutItemKey> {
        if let Some(key) = self.next
            && matches_item(tree, key, desired, text)
        {
            return Some(key);
        }

        let index = self.index.get_or_insert_with(|| {
            let mut index = Candidates::default();

            for key in tree.items.cursor(self.next) {
                index.entry(tree.items[key].ty).or_default().push_back(key);
            }

            index
        });
        let candidates = index.get_mut(&desired.ty)?;

        while let Some(key) = candidates.pop_front() {
            if !claimed.contains(&key)
                && tree.items.parent(key) == Some(self.key)
                && matches_item(tree, key, desired, text)
            {
                return Some(key);
            }
        }

        None
    }

    fn adopt(
        &mut self,
        tree: &LayoutTree,
        desired: LayoutItem,
        text: Option<NodeKey>,
        claimed: &FxHashSet<LayoutItemKey>,
    ) -> Option<LayoutItemKey> {
        let anonymous = item(LayoutItemTy::AnonymousBlock, ChildLayoutMode::Inline);
        let index = self.adoption.get_or_insert_with(|| {
            let mut index = Candidates::default();

            for key in tree.items.cursor(self.next) {
                if tree.items[key] == anonymous {
                    for child in tree.items.children(key) {
                        index
                            .entry(tree.items[child].ty)
                            .or_default()
                            .push_back(child);
                    }
                }

                index.entry(tree.items[key].ty).or_default().push_back(key);
            }

            index
        });
        let candidates = index.get_mut(&desired.ty)?;

        while let Some(key) = candidates.pop_front() {
            let parent = tree.items.parent(key);
            let eligible = parent == Some(self.key)
                || parent.is_some_and(|parent| {
                    !claimed.contains(&parent)
                        && tree.items.parent(parent) == Some(self.key)
                        && tree.items.get(parent) == Some(&anonymous)
                });

            if eligible && !claimed.contains(&key) && matches_item(tree, key, desired, text) {
                return Some(key);
            }
        }

        None
    }

    fn insert(&self, tree: &mut LayoutTree, key: LayoutItemKey) {
        if let Some(next) = self.next {
            tree.items.before(next, key);
        } else {
            tree.items.append(self.key, key);
        }
    }
}

#[derive(Clone, Copy)]
struct Flow {
    frame: usize,
    span_start: usize,
    inline_run: bool,
}

fn item(ty: LayoutItemTy, child_layout: ChildLayoutMode) -> LayoutItem {
    LayoutItem {
        ty,
        child_layout,
        continuation: Default::default(),
    }
}

fn replace_item(tree: &mut LayoutTree, key: LayoutItemKey, desired: LayoutItem) {
    tree.unlink_continuation(key);
    tree.items[key] = desired;
}

fn matches_item(
    tree: &LayoutTree,
    key: LayoutItemKey,
    desired: LayoutItem,
    text: Option<NodeKey>,
) -> bool {
    tree.items.get(key).is_some_and(|old| old.ty == desired.ty)
        && text.is_none_or(|node| {
            tree.items
                .first_child(key)
                .and_then(|child| tree.items.get(child))
                .is_some_and(|item| item.ty == LayoutItemTy::Text(node))
        })
}

fn update_item(
    tree: &mut LayoutTree,
    target: Option<LayoutItemKey>,
    desired: LayoutItem,
) -> LayoutItemKey {
    if let Some(key) = target.filter(|&key| matches_item(tree, key, desired, None)) {
        replace_item(tree, key, desired);

        return key;
    }

    let key = tree.add_item(desired);

    if let Some(old) = target {
        tree.items.before(old, key);
        tree.delete_item(old);
    }

    key
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
