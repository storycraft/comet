mod validate;

use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use rustc_hash::FxHashSet;

use crate::{
    style::{DisplayInner, DisplayOuter},
    tree::{InlineIns, InlineLayoutItem, LayoutItem, LayoutItemKey, LayoutItemTy, LayoutTree},
};

/// Updates an existing layout subtree directly while traversing its source UI.
/// Matching source identities and item kinds retain their keys. Display modes
/// determine flow boundaries; retained text and instruction buffers are reused.
/// Scratch storage is cleared between calls without releasing its allocations.
#[derive(Default)]
pub struct LayoutTreeBuilder {
    pending: Vec<Task>,
    flows: Vec<Flow>,
    spans: Vec<NodeKey>,
    scan: Vec<NodeKey>,
    boxes: FxHashSet<LayoutItemKey>,
}

impl LayoutTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update at `target`, replacing stale, malformed, or incompatible items.
    /// Unrelated subtrees are untouched. A missing source deletes the target.
    /// Choose a complete flow boundary: changes affecting surrounding inline
    /// runs must update their enclosing container. Inline-block bodies are also
    /// supported boundaries. Always use the returned root key.
    pub fn build(
        &mut self,
        ui: &Ui,
        root: NodeKey,
        tree: &mut LayoutTree,
        target: LayoutItemKey,
    ) -> Option<LayoutItemKey> {
        self.update(ui, root, tree, Some(target))
    }

    /// Build a detached root using the same traversal as incremental updates.
    /// Missing display properties use their defaults, including for text nodes.
    /// Split inline roots receive an anonymous block container.
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
        self.clear_scratch();
        if ui.node(root).is_none() {
            if let Some(target) = target {
                tree.delete_item(target);
            }
            return None;
        }

        let atomic_body = target.is_some_and(|target| {
            tree.items
                .parent(target)
                .and_then(|parent| tree.items.get(parent))
                .is_some_and(|item| {
                    matches!(&item.ty, LayoutItemTy::Inline(inline)
                    if inline.instructions.contains(&InlineIns::InlineBlock(target)))
                })
        });
        let (outer, inner) = display(ui, root);
        let block_root = atomic_body || outer == DisplayOuter::Block;
        let split_root =
            !block_root && inner == DisplayInner::Flow && self.contains_block(ui, root);
        let source = (!split_root).then_some(root);
        let is_inline = !block_root && !split_root;

        let key = match target {
            Some(key)
                if matches_item(tree, key, source, is_inline)
                    && validate::valid(ui, tree, key, &mut self.boxes) =>
            {
                key
            }
            _ => {
                let key = tree.add_item(new_item(source, is_inline));
                if let Some(target) = target {
                    replace_root(tree, target, key);
                }
                key
            }
        };

        if is_inline {
            // The unsplit root already is the run, so it needs no wrapper.
            let next_box = tree.items.first_child(key);
            if let Some(content) = inline(tree, key) {
                content.text.clear();
                content.instructions.clear();
            }
            self.flows.push(Flow {
                parent: None,
                next_child: None,
                run: Some(key),
                next_box,
                span_start: 0,
                interrupted: false,
            });
        } else {
            self.push_flow(tree, key);
        }

        self.pending.push(if block_root {
            Task::Content(root)
        } else {
            Task::Visit(root)
        });
        let result = self
            .traverse(ui, tree)
            .and_then(|()| self.finish_flow(tree));
        self.clear_scratch();

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
                Task::PopInline { empty } => {
                    if empty || self.flows.last()?.interrupted {
                        self.ensure_run(ui, tree)?;
                    }
                    if let Some(key) = self.flows.last()?.run {
                        inline(tree, key)?.instructions.push(InlineIns::PopInline);
                    }
                    self.spans.pop()?;
                }
                Task::PopBlock => self.finish_flow(tree)?,
                Task::Visit(node) => {
                    ui.node(node)?;
                    let (outer, inner) = display(ui, node);

                    match outer {
                        DisplayOuter::Block => {
                            let flow = self.flows.last()?;
                            if self.spans.len() > flow.span_start && !flow.interrupted {
                                self.ensure_run(ui, tree)?;
                            }
                            self.finish_run(tree)?;
                            let flow = self.flows.last_mut()?;
                            flow.interrupted = self.spans.len() > flow.span_start;
                            let key = select_child(
                                ui,
                                tree,
                                flow.parent?,
                                &mut flow.next_child,
                                Some(node),
                                false,
                                &mut self.boxes,
                            );
                            self.push_flow(tree, key);
                            self.pending.push(Task::PopBlock);
                        }
                        DisplayOuter::Inline if inner != DisplayInner::Flow => {
                            let run = self.ensure_run(ui, tree)?;
                            let flow = self.flows.last_mut()?;
                            let key = select_child(
                                ui,
                                tree,
                                run,
                                &mut flow.next_box,
                                Some(node),
                                false,
                                &mut self.boxes,
                            );
                            inline(tree, run)?
                                .instructions
                                .push(InlineIns::InlineBlock(key));
                            self.push_flow(tree, key);
                            self.pending.push(Task::PopBlock);
                        }
                        DisplayOuter::Inline => {
                            if self.flows.last()?.interrupted {
                                self.ensure_run(ui, tree)?;
                            }
                            self.spans.push(node);
                            if let Some(key) = self.flows.last()?.run {
                                inline(tree, key)?
                                    .instructions
                                    .push(InlineIns::PushInline(node));
                            }
                            self.pending.push(Task::PopInline {
                                empty: ui.first_child(node).is_none(),
                            });
                        }
                    }

                    self.write_content(ui, tree, node, inner)?;
                }
                Task::Content(node) => self.write_content(ui, tree, node, display(ui, node).1)?,
            }
        }
        Some(())
    }

    fn write_content(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutTree,
        node: NodeKey,
        inner: DisplayInner,
    ) -> Option<()> {
        if *ui.node(node)? == Node::Text {
            let key = self.ensure_run(ui, tree)?;
            let content = inline(tree, key)?;
            let start = content.text.len();
            if let Some(text) = ui.prop::<Text>(node) {
                content.text.push_str(&text.0);
            }
            content.instructions.push(InlineIns::Text {
                node,
                range: start..content.text.len(),
            });
        }

        if inner != DisplayInner::Content {
            self.pending
                .extend(ui.children(node).rev().map(Task::Visit));
        }
        Some(())
    }

    fn ensure_run(&mut self, ui: &Ui, tree: &mut LayoutTree) -> Option<LayoutItemKey> {
        let flow = self.flows.last_mut()?;
        if let Some(key) = flow.run {
            return Some(key);
        }

        let key = select_child(
            ui,
            tree,
            flow.parent?,
            &mut flow.next_child,
            None,
            true,
            &mut self.boxes,
        );
        flow.next_box = tree.items.first_child(key);
        let content = inline(tree, key)?;
        content.text.clear();
        content.instructions.clear();
        content.instructions.extend(
            self.spans
                .get(flow.span_start..)?
                .iter()
                .copied()
                .map(InlineIns::PushInline),
        );
        flow.run = Some(key);
        flow.interrupted = false;
        Some(key)
    }

    fn finish_run(&mut self, tree: &mut LayoutTree) -> Option<()> {
        let flow = self.flows.last_mut()?;
        if let Some(key) = flow.run.take() {
            inline(tree, key)?.instructions.extend(
                self.spans
                    .get(flow.span_start..)?
                    .iter()
                    .map(|_| InlineIns::PopInline),
            );
            delete_remaining(tree, flow.next_box.take());
        }
        Some(())
    }

    fn push_flow(&mut self, tree: &LayoutTree, parent: LayoutItemKey) {
        self.flows.push(Flow {
            parent: Some(parent),
            next_child: tree.items.first_child(parent),
            run: None,
            next_box: None,
            span_start: self.spans.len(),
            interrupted: false,
        });
    }

    fn finish_flow(&mut self, tree: &mut LayoutTree) -> Option<()> {
        self.finish_run(tree)?;
        let flow = self.flows.pop()?;
        delete_remaining(tree, flow.next_child);
        self.spans.truncate(flow.span_start);
        Some(())
    }

    // Only inline roots need this topology lookahead. Atomic boxes stop the scan.
    fn contains_block(&mut self, ui: &Ui, root: NodeKey) -> bool {
        self.scan.extend(ui.children(root));
        while let Some(node) = self.scan.pop() {
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

    fn clear_scratch(&mut self) {
        self.pending.clear();
        self.flows.clear();
        self.spans.clear();
        self.scan.clear();
        self.boxes.clear();
    }
}

enum Task {
    Visit(NodeKey),
    Content(NodeKey),
    PopInline { empty: bool },
    PopBlock,
}

// Cursors over unconsumed children. Inline ancestors share the span stack.
struct Flow {
    parent: Option<LayoutItemKey>,
    next_child: Option<LayoutItemKey>,
    run: Option<LayoutItemKey>,
    next_box: Option<LayoutItemKey>,
    span_start: usize,
    interrupted: bool,
}

// Consume a matching existing child, moving it only when its order changed.
// Other children stay available until the container traversal finishes.
fn select_child(
    ui: &Ui,
    tree: &mut LayoutTree,
    parent: LayoutItemKey,
    cursor: &mut Option<LayoutItemKey>,
    node: Option<NodeKey>,
    is_inline: bool,
    boxes: &mut FxHashSet<LayoutItemKey>,
) -> LayoutItemKey {
    let mut candidate = *cursor;
    while let Some(key) = candidate {
        if matches_item(tree, key, node, is_inline) {
            if validate::valid(ui, tree, key, boxes) {
                if Some(key) == *cursor {
                    *cursor = tree.items.next_sibling(key);
                } else if let Some(next) = *cursor {
                    tree.items.before(next, key);
                }
                return key;
            }

            if Some(key) == *cursor {
                *cursor = tree.items.next_sibling(key);
            }
            tree.delete_item(key);
            break;
        }
        candidate = tree.items.next_sibling(key);
    }

    let key = tree.add_item(new_item(node, is_inline));
    if let Some(next) = *cursor {
        tree.items.before(next, key);
    } else {
        tree.items.append(parent, key);
    }
    key
}

fn replace_root(tree: &mut LayoutTree, target: LayoutItemKey, replacement: LayoutItemKey) {
    if tree.items.get(target).is_none() {
        return;
    }
    tree.items.before(target, replacement);
    if let Some(parent) = tree.items.parent(target)
        && let Some(inline) = inline(tree, parent)
    {
        for ins in &mut inline.instructions {
            if let InlineIns::InlineBlock(key) = ins
                && *key == target
            {
                *key = replacement;
            }
        }
    }
    tree.delete_item(target);
}

fn delete_remaining(tree: &mut LayoutTree, mut next: Option<LayoutItemKey>) {
    while let Some(key) = next {
        next = tree.items.next_sibling(key);
        tree.delete_item(key);
    }
}

fn matches_item(
    tree: &LayoutTree,
    key: LayoutItemKey,
    node: Option<NodeKey>,
    is_inline: bool,
) -> bool {
    tree.items.get(key).is_some_and(|item| {
        item.node == node && matches!(item.ty, LayoutItemTy::Inline(_)) == is_inline
    })
}

fn new_item(node: Option<NodeKey>, is_inline: bool) -> LayoutItem {
    LayoutItem {
        node,
        ty: if is_inline {
            LayoutItemTy::Inline(InlineLayoutItem::default())
        } else {
            LayoutItemTy::Block
        },
    }
}

fn display(ui: &Ui, node: NodeKey) -> (DisplayOuter, DisplayInner) {
    (
        ui.prop::<DisplayOuter>(node)
            .map(|value| *value)
            .unwrap_or_default(),
        ui.prop::<DisplayInner>(node)
            .map(|value| *value)
            .unwrap_or_default(),
    )
}

fn inline(tree: &mut LayoutTree, key: LayoutItemKey) -> Option<&mut InlineLayoutItem> {
    match &mut tree.items.get_mut(key)?.ty {
        LayoutItemTy::Inline(inline) => Some(inline),
        LayoutItemTy::Block => None,
    }
}
