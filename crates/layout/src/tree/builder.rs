mod candidates;
mod flows;

use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};
use rustc_hash::FxHashMap;

use self::{candidates::Candidates, flows::Flows};

use crate::{
    style::bx::{DisplayInner, DisplayOuter},
    tree::{ChildLayoutMode, LayoutItem, LayoutItemFlags, LayoutItemKey, LayoutItemTy, LayoutTree},
};

#[derive(Default)]
pub struct LayoutTreeBuilder {
    pending: Vec<Task>,
    frames: Vec<Frame>,
    flows: Flows,
    scan: Vec<NodeKey>,

    candidates: Candidates,
    last_items: FxHashMap<NodeKey, LayoutItemKey>,
}

impl LayoutTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a new tree with `None`, or update the supplied root with `Some(key)`.
    /// Mark changed items before updating. Clean roots return immediately.
    /// The source and layout root must describe the same tree; descendant targets
    /// are not resolved to an owning root. Use `item_for_node` for child results.
    pub fn build(
        &mut self,
        ui: &Ui,
        root: NodeKey,
        tree: &mut LayoutTree,
        target: Option<LayoutItemKey>,
    ) -> Option<LayoutItemKey> {
        if let Some(key) = target
            && tree
                .items
                .get(key)
                .is_some_and(|item| item.flags.is_empty())
        {
            return Some(key);
        }

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

        let outer = display_outer(ui, root);
        let inner = ui.prop::<DisplayInner>(root);
        let split = is_inline_flow(outer, inner.as_deref()) && self.contains_block(ui, root);

        let desired = if split {
            item(LayoutItemTy::AnonymousBlock, ChildLayoutMode::Block)
        } else {
            self.source_item(ui, root, outer, inner.as_deref())
        };

        let key = if let Some(key) = target.filter(|&key| {
            tree.items
                .get(key)
                .is_some_and(|item| !item.flags.contains(LayoutItemFlags::DIRTY))
        }) {
            tree.unmap_item(key);
            replace_item(tree, key, desired);

            key
        } else {
            update_item(tree, target, desired)
        };
        self.record_item(tree, key);

        self.push_frame(tree, key);
        self.flows.push(tree, &mut self.frames);

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
                    if self.reuse_clean_child(tree, node) {
                        continue;
                    }

                    if *ui.node(node)? == Node::Text {
                        self.resume_spans(tree)?;
                        self.text_child(tree, node)?;

                        continue;
                    }

                    let outer = display_outer(ui, node);
                    let inner = ui.prop::<DisplayInner>(node);

                    if outer == DisplayOuter::Block {
                        let base = self.flows.current_frame()?;

                        while self.frames.len() > base + 1 {
                            self.finish_frame(tree)?;
                        }

                        self.flows.set_inline_run(false)?;
                    } else {
                        self.resume_spans(tree)?;
                    }

                    let desired = self.source_item(ui, node, outer, inner.as_deref());
                    let (key, clean) = self.child(tree, desired)?;

                    // Split spans must replay their flow interruptions. Unsplit clean
                    // items retain their entire subtree and continuation links.
                    if clean && !self.frames.last()?.force {
                        continue;
                    }

                    self.push_frame(tree, key);

                    if is_inline_flow(outer, inner.as_deref()) {
                        self.flows.push_span(node);
                        self.pending.push(Task::EndInline);
                    } else {
                        self.flows.push(tree, &mut self.frames);
                        self.pending.push(Task::EndFlow);
                    }

                    self.pending.push(Task::Content(node));
                }

                Task::Content(node) => {
                    let mode = tree.items[self.frames.last()?.key].child_layout;

                    if mode == ChildLayoutMode::Container {
                        self.pending
                            .extend(ui.children(node).rev().map(Task::ContainerChild));
                    } else if mode != ChildLayoutMode::None {
                        self.pending
                            .extend(ui.children(node).rev().map(Task::Visit));
                    }
                }

                Task::ContainerChild(node) => {
                    let outer = display_outer(ui, node);
                    let inner = ui.prop::<DisplayInner>(node);

                    if outer == DisplayOuter::Inline {
                        // Each source child owns a separate formatting context,
                        // including any block interruptions inside an inline.
                        let mode = if *ui.node(node)? != Node::Text
                            && is_inline_flow(outer, inner.as_deref())
                            && self.contains_block(ui, node)
                        {
                            ChildLayoutMode::Block
                        } else {
                            ChildLayoutMode::Inline
                        };
                        let (key, _) =
                            self.child(tree, item(LayoutItemTy::AnonymousBlock, mode))?;

                        self.push_frame(tree, key);
                        self.flows.push(tree, &mut self.frames);
                        self.pending.push(Task::EndFlow);
                    }

                    self.pending.push(Task::Visit(node));
                }

                Task::EndInline => {
                    // Retain empty trailing fragments as well as nonempty ones.
                    self.resume_spans(tree)?;
                    self.finish_frame(tree)?;
                    self.flows.pop_span()?;
                }

                Task::EndFlow => self.finish_flow(tree)?,
            }
        }

        Some(())
    }

    /// Consume an unchanged next sibling without reading its UI properties or children.
    fn reuse_clean_child(&mut self, tree: &mut LayoutTree, node: NodeKey) -> bool {
        let Some(frame) = self.frames.last_mut() else {
            return false;
        };

        let Some(key) = frame.next else {
            return false;
        };

        let item = tree.items[key];

        if frame.force
            || !item.flags.is_empty()
            || item.ty.node() != Some(node)
            || item.link != Default::default()
            || (tree.items[frame.key].child_layout == ChildLayoutMode::Block
                && !matches!(item.ty, LayoutItemTy::Block(_)))
        {
            return false;
        }

        frame.next = tree.items.next_sibling(key);
        self.candidates.claim(key);
        self.record_item(tree, key);

        true
    }

    fn source_item(
        &mut self,
        ui: &Ui,
        node: NodeKey,
        outer: DisplayOuter,
        inner: Option<&DisplayInner>,
    ) -> LayoutItem {
        let ty = if outer == DisplayOuter::Block {
            LayoutItemTy::Block(node)
        } else if is_inline_flow(outer, inner) {
            LayoutItemTy::Inline(node)
        } else {
            LayoutItemTy::AtomicInline(node)
        };

        let child_layout = if matches!(ty, LayoutItemTy::Inline(_)) {
            ChildLayoutMode::Inline
        } else {
            self.content_mode(ui, node, inner)
        };

        item(ty, child_layout)
    }

    fn update_text(
        &mut self,
        tree: &mut LayoutTree,
        node: NodeKey,
        target: Option<LayoutItemKey>,
    ) -> Option<LayoutItemKey> {
        let desired = item(LayoutItemTy::Text(node), ChildLayoutMode::None);
        let key = update_item(tree, target, desired);

        self.record_item(tree, key);
        finish_text(tree, key);

        Some(key)
    }

    fn text_child(&mut self, tree: &mut LayoutTree, node: NodeKey) -> Option<LayoutItemKey> {
        // resume_spans has already established the containing inline run.
        let (key, _) = self.child(tree, item(LayoutItemTy::Text(node), ChildLayoutMode::None))?;

        finish_text(tree, key);

        Some(key)
    }

    fn resume_spans(&mut self, tree: &mut LayoutTree) -> Option<()> {
        if self.flows.needs_inline_run(tree, &self.frames)? {
            let (key, _) = self.child(
                tree,
                item(LayoutItemTy::AnonymousBlock, ChildLayoutMode::Inline),
            )?;

            self.push_frame(tree, key);
            self.flows.set_inline_run(true)?;
        }

        let mut index = self.flows.resume_span_start(self.frames.len())?;

        while let Some(node) = self.flows.span(index) {
            let (key, _) = self.child(
                tree,
                item(LayoutItemTy::Inline(node), ChildLayoutMode::Inline),
            )?;

            self.push_frame(tree, key);
            index += 1;
        }

        Some(())
    }

    fn child(
        &mut self,
        tree: &mut LayoutTree,
        desired: LayoutItem,
    ) -> Option<(LayoutItemKey, bool)> {
        let frame = self.frames.last_mut()?;
        let candidate = if let Some(key) = self.candidates.find(tree, frame, desired) {
            Some(key)
        } else if let Some(frame) = self.flows.inline_parent(self.frames.len()) {
            // A new inline run can adopt items from the previous direct child list.
            let parent = &mut self.frames[frame];
            let key = self.candidates.adopt(tree, parent, desired);

            if let Some(key) = key
                && parent.next == Some(key)
            {
                parent.next = tree.items.next_sibling(key);
            }

            key
        } else {
            None
        };

        let (key, clean) = if let Some(key) = candidate {
            let old = tree.items[key];
            let clean = old.flags.is_empty()
                && old.child_layout == desired.child_layout
                && old.link == Default::default();

            replace_item(tree, key, desired);

            (key, clean)
        } else {
            let key = tree.add_item(desired);

            tree.items[key].flags.insert(LayoutItemFlags::DIRTY);

            (key, false)
        };

        let frame = self.frames.last_mut()?;

        if frame.next == Some(key) {
            frame.next = tree.items.next_sibling(key);
        } else {
            frame.insert(tree, key);
        }

        self.candidates.claim(key);
        self.record_item(tree, key);

        Some((key, clean))
    }

    fn record_item(&mut self, tree: &mut LayoutTree, key: LayoutItemKey) {
        let ty = tree.items[key].ty;

        if let Some(node) = ty.node() {
            if let Some(prev) = self.last_items.insert(node, key) {
                if matches!(ty, LayoutItemTy::Inline(_)) {
                    tree.items[prev].link.next = Some(key);
                    tree.items[key].link.prev = Some(prev);
                }
            } else {
                tree.map_first_item(node, key);
            }
        }
    }

    fn push_frame(&mut self, tree: &mut LayoutTree, key: LayoutItemKey) {
        let force = self.frames.last().is_some_and(|frame| frame.force)
            || tree.items[key].flags.contains(LayoutItemFlags::DIRTY);

        tree.items[key].flags = LayoutItemFlags::empty();
        self.frames.push(Frame {
            key,
            next: tree.items.first_child(key),
            index: None,
            adoption: None,
            force,
        });
    }

    fn finish_frame(&mut self, tree: &mut LayoutTree) -> Option<()> {
        let promote = self.flows.inline_parent(self.frames.len()).is_some();
        let frame = self.frames.pop()?;

        if promote {
            // Promote the tail backwards so prepending preserves sibling and candidate order.
            let parent = self.frames.last_mut()?;
            let mut next = frame.next.and_then(|_| tree.items.last_child(frame.key));

            while let Some(key) = next {
                next = if Some(key) == frame.next {
                    None
                } else {
                    tree.items.prev_sibling(key)
                };

                parent.insert(tree, key);
                parent.next = Some(key);

                if let Some(index) = parent.index {
                    self.candidates.push(index, tree.items[key].ty, key, true);
                }

                if let Some(index) = parent.adoption {
                    self.candidates.push(index, tree.items[key].ty, key, true);
                }
            }
        } else {
            delete_remaining(tree, frame.next);
        }

        Some(())
    }

    fn finish_flow(&mut self, tree: &mut LayoutTree) -> Option<()> {
        let frame = self.flows.pop()?;

        while self.frames.len() > frame {
            self.finish_frame(tree)?;
        }

        Some(())
    }

    fn contains_block(&mut self, ui: &Ui, root: NodeKey) -> bool {
        self.scan.extend(ui.children(root));

        while let Some(node) = self.scan.pop() {
            if ui.node(node).is_some_and(|node| *node == Node::Text) {
                continue;
            }

            let outer = display_outer(ui, node);

            if outer == DisplayOuter::Block {
                self.scan.clear();

                return true;
            }

            let inner = ui.prop::<DisplayInner>(node);

            if is_inline_flow(outer, inner.as_deref()) {
                self.scan.extend(ui.children(node));
            }
        }

        false
    }

    fn content_mode(
        &mut self,
        ui: &Ui,
        node: NodeKey,
        inner: Option<&DisplayInner>,
    ) -> ChildLayoutMode {
        match inner {
            Some(DisplayInner::Content) => ChildLayoutMode::None,
            Some(DisplayInner::Container(_)) => ChildLayoutMode::Container,
            _ if ui.children(node).next().is_none() || self.contains_block(ui, node) => {
                ChildLayoutMode::Block
            }
            _ => ChildLayoutMode::Inline,
        }
    }

    fn clear(&mut self) {
        self.pending.clear();
        self.frames.clear();
        self.flows.clear();
        self.scan.clear();
        self.candidates.clear();
        self.last_items.clear();
    }
}

enum Task {
    Visit(NodeKey),
    ContainerChild(NodeKey),
    Content(NodeKey),
    EndInline,
    EndFlow,
}

struct Frame {
    key: LayoutItemKey,
    next: Option<LayoutItemKey>,
    index: Option<usize>,
    adoption: Option<usize>,
    force: bool,
}

impl Frame {
    fn insert(&self, tree: &mut LayoutTree, key: LayoutItemKey) {
        if let Some(next) = self.next {
            tree.items.before(next, key);
        } else {
            tree.items.append(self.key, key);
        }
    }
}

fn item(ty: LayoutItemTy, child_layout: ChildLayoutMode) -> LayoutItem {
    LayoutItem {
        ty,
        child_layout,
        link: Default::default(),
        flags: Default::default(),
    }
}

fn replace_item(tree: &mut LayoutTree, key: LayoutItemKey, desired: LayoutItem) {
    let mut flags = tree.items[key].flags;

    if tree.items[key].ty != desired.ty || tree.items[key].child_layout != desired.child_layout {
        flags.insert(LayoutItemFlags::DIRTY);
    }

    tree.unlink_continuation(key);
    tree.items[key] = desired;
    tree.items[key].flags = flags;
}

fn matches_item(tree: &LayoutTree, key: LayoutItemKey, desired: LayoutItem) -> bool {
    tree.items.get(key).is_some_and(|old| old.ty == desired.ty)
}

fn update_item(
    tree: &mut LayoutTree,
    target: Option<LayoutItemKey>,
    desired: LayoutItem,
) -> LayoutItemKey {
    if let Some(key) = target.filter(|&key| matches_item(tree, key, desired)) {
        replace_item(tree, key, desired);

        return key;
    }

    let key = tree.add_item(desired);
    tree.items[key].flags.insert(LayoutItemFlags::DIRTY);

    if let Some(old) = target {
        tree.items.before(old, key);
        tree.delete_item(old);
    }

    key
}

fn display_outer(ui: &Ui, node: NodeKey) -> DisplayOuter {
    if ui.node(node).is_some_and(|node| *node == Node::Text) {
        return DisplayOuter::Inline;
    }

    ui.prop::<DisplayOuter>(node)
        .map(|v| *v)
        .unwrap_or_default()
}

fn is_inline_flow(outer: DisplayOuter, inner: Option<&DisplayInner>) -> bool {
    outer == DisplayOuter::Inline && matches!(inner, None | Some(DisplayInner::Flow))
}

fn finish_text(tree: &mut LayoutTree, key: LayoutItemKey) {
    tree.items[key].flags = LayoutItemFlags::empty();

    delete_remaining(tree, tree.items.first_child(key));
}

fn delete_remaining(tree: &mut LayoutTree, mut next: Option<LayoutItemKey>) {
    while let Some(key) = next {
        next = tree.items.next_sibling(key);
        tree.delete_item(key);
    }
}

fn same_shape(tree: &LayoutTree, key: LayoutItemKey, desired: LayoutItem) -> bool {
    tree.items
        .get(key)
        .is_some_and(|item| item.ty == desired.ty && item.child_layout == desired.child_layout)
}
