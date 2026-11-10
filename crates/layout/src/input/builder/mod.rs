use std::sync::Arc;

use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};

use crate::{
    input::{
        ChildLayout, LayoutFlags, LayoutInputItem, LayoutInputKey, LayoutInputKind, TextStyle,
        tree::LayoutInputTree,
    },
    parley::{conv, style},
    style::{
        bx::{DisplayInner, DisplayOuter},
        geometry::{Size, UnitResolveContext},
    },
};

/// Builds a [`LayoutInputTree`] that mirrors the source UI 1:1.
#[derive(Default)]
pub struct LayoutInputTreeBuilder {
    pending: Vec<Task>,
}

impl LayoutInputTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a new tree for `root` with `None`, or rebuild the item generated for
    /// `root` with `Some(key)`. Clean targets return immediately; mark changed
    /// nodes before rebuilding.
    ///
    /// Returns `target` when it is rebuilt in place, or the newly created key.
    /// A target generated for a different node is replaced by a new item at its
    /// position. Returns `None` and deletes `target` if `root` is not in the UI,
    /// even when `target` is clean.
    ///
    /// Generated items have every [`LayoutFlags`] set. Rebuilding re-resolves the
    /// whole target subtree, reusing keys of items whose node stays under the same
    /// parent, and marks every rebuilt item dirty. Items only describe their own
    /// node, so any changed node can be rebuilt on its own. Flags are left for the
    /// consumer of the input tree to clear. Use `item_for_node` for child results.
    pub fn build(
        &mut self,
        ui: &Ui,
        root: NodeKey,
        root_font_size: f64,
        viewport: Size<f64>,
        tree: &mut LayoutInputTree,
        target: Option<LayoutInputKey>,
    ) -> Option<LayoutInputKey> {
        let target = target.filter(|&key| tree.items.get(key).is_some());

        if ui.kind(root).is_none() {
            if let Some(key) = target {
                let parent = tree.items.parent(key);

                tree.delete_item(key);

                if let Some(parent) = parent {
                    tree.mark_dirty(parent);
                }
            }

            return None;
        }

        if let Some(key) = target
            && tree.items[key].flags.is_empty()
        {
            return Some(key);
        }

        self.clear();

        let cx = UnitResolveContext {
            percentage_basis: root_font_size,
            root_font_size,
            font_size: root_font_size,
            viewport,
        };

        let parent_style = match target.and_then(|key| tree.items.parent(key)) {
            Some(parent) => tree.items[parent].text_style.clone(),
            None => match ui.parent(root) {
                Some(parent) => style::resolve_inherited(ui, parent, &cx),
                None => Arc::new(style::root_text_style(root_font_size)),
            },
        };

        let existing = target.filter(|&key| tree.items[key].node == root);
        let key = self.update(ui, tree, root, existing, &parent_style, &cx);

        if existing.is_none()
            && let Some(old) = target
        {
            tree.items.before(old, key);
            tree.delete_item(old);
        }

        // Propagate to the ancestors outside of the rebuilt subtree.
        tree.mark_dirty(key);
        self.pending.push(Task { key, node: root });

        while let Some(task) = self.pending.pop() {
            self.reconcile_children(ui, tree, task, &cx);
        }

        self.clear();

        Some(key)
    }

    /// Match the item's children to the source children, in order, and rebuild them.
    fn reconcile_children(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        Task { key, node }: Task,
        cx: &UnitResolveContext,
    ) {
        let mut next = tree.items.first_child(key);

        if tree.items[key].child_layout != ChildLayout::None {
            let parent_style = tree.items[key].text_style.clone();

            for child in ui.children(node) {
                let existing = tree
                    .item_for_node(child)
                    .filter(|&item| tree.items.parent(item) == Some(key));
                let child_key = self.update(ui, tree, child, existing, &parent_style, cx);

                if next == Some(child_key) {
                    next = tree.items.next_sibling(child_key);
                } else if let Some(next) = next {
                    tree.items.before(next, child_key);
                } else {
                    tree.items.append(key, child_key);
                }

                tree.mark_dirty(child_key);
                self.pending.push(Task {
                    key: child_key,
                    node: child,
                });
            }
        }

        while let Some(item) = next {
            next = tree.items.next_sibling(item);
            tree.delete_item(item);
        }
    }

    /// Resolve the source node and write it into `existing` or a new item.
    fn update(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        node: NodeKey,
        existing: Option<LayoutInputKey>,
        parent_style: &Arc<TextStyle>,
        cx: &UnitResolveContext,
    ) -> LayoutInputKey {
        let (kind, child_layout) = classify(ui, node);
        let text_style = conv::resolve_text_style(ui, node, cx, parent_style);

        let Some(key) = existing else {
            return tree.add_item(LayoutInputItem {
                kind,
                node,
                text_style,
                child_layout,
                flags: LayoutFlags::all(),
            });
        };

        let item = &mut tree.items[key];
        item.kind = kind;
        item.text_style = text_style;
        item.child_layout = child_layout;

        key
    }

    fn clear(&mut self) {
        self.pending.clear();
    }
}

struct Task {
    key: LayoutInputKey,
    node: NodeKey,
}

/// Derive the item kind and child layout from the node's own display.
/// Splitting inline content around blocks is left to later stages.
fn classify(ui: &Ui, node: NodeKey) -> (LayoutInputKind, ChildLayout) {
    if ui.kind(node) == Some(Node::Text) {
        return (LayoutInputKind::Text, ChildLayout::None);
    }

    let outer = display_outer(ui, node);
    let inner = ui.prop::<DisplayInner>(node);
    let inner = inner.as_deref();

    let kind = match (outer, inner) {
        (DisplayOuter::Block, _) => LayoutInputKind::Block,
        (DisplayOuter::Inline, None | Some(DisplayInner::Flow)) => LayoutInputKind::Inline,
        // Inline-level boxes establishing their own context are atomic.
        (DisplayOuter::Inline, _) => LayoutInputKind::AtomicInline,
    };

    let child_layout = match inner {
        Some(DisplayInner::Content) => ChildLayout::None,
        Some(DisplayInner::Container(_)) => ChildLayout::Container,
        // Inline flow children participate in the surrounding inline formatting context.
        _ if kind == LayoutInputKind::Inline => ChildLayout::Inline,
        _ => ChildLayout::Block,
    };

    (kind, child_layout)
}

fn display_outer(ui: &Ui, node: NodeKey) -> DisplayOuter {
    if ui.kind(node).is_some_and(|node| node == Node::Text) {
        return DisplayOuter::Inline;
    }

    ui.prop::<DisplayOuter>(node)
        .map(|v| *v)
        .unwrap_or_default()
}
