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
    /// Re-resolve every item in the current build, as if it were generated.
    force: bool,
}

impl LayoutInputTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a new tree for `root` with `None`, or update the item generated for
    /// `root` with `Some(key)`. Clean targets return immediately; mark changed
    /// nodes before updating.
    ///
    /// Returns `target` when it is updated in place, or the newly created key.
    /// A target generated for a different node is replaced by a new item at its
    /// position. Returns `None` and deletes `target` if `root` is not in the UI,
    /// even when `target` is clean.
    ///
    /// Generated items have every [`LayoutFlags`] set. Updating visits flagged
    /// items and keeps clean subtrees as they are, unless an inherited text style
    /// changed. Existing items are only marked dirty when their contents or child
    /// lists change. Flags are left for the consumer of the input tree to clear.
    /// Items only describe their own node, so any changed node can be updated on
    /// its own. Use `item_for_node` for child results.
    ///
    /// With `force`, the whole target subtree is re-resolved and every item is
    /// flagged as if it were newly generated, even when the target is clean. Keys
    /// are still reused. Use it when the root font size or the viewport changes,
    /// since every item may depend on them.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        ui: &Ui,
        root: NodeKey,
        root_font_size: f64,
        viewport: Size<f64>,
        tree: &mut LayoutInputTree,
        target: Option<LayoutInputKey>,
        force: bool,
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

        if !force
            && let Some(key) = target
            && tree.items[key].flags.is_empty()
        {
            return Some(key);
        }

        self.clear();
        self.force = force;

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
        let update = self.update(ui, tree, root, existing, &parent_style, &cx);

        if existing.is_none()
            && let Some(old) = target
        {
            tree.items.before(old, update.key);
            tree.delete_item(old);
            // Propagate the new item's flags to the ancestors.
            tree.mark_dirty(update.key);
        } else if force {
            tree.mark_dirty(update.key);
        }

        if update.descend {
            self.pending.push(Task {
                key: update.key,
                node: root,
                force: update.force,
            });
        }

        while let Some(task) = self.pending.pop() {
            self.reconcile_children(ui, tree, task, &cx);
        }

        self.clear();

        Some(update.key)
    }

    /// Match the item's children to the source children, in order.
    fn reconcile_children(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        Task { key, node, force }: Task,
        cx: &UnitResolveContext,
    ) {
        let mut next = tree.items.first_child(key);
        let mut changed = false;

        if tree.items[key].child_layout != ChildLayout::None {
            let parent_style = tree.items[key].text_style.clone();

            for child in ui.children(node) {
                let existing = tree
                    .item_for_node(child)
                    .filter(|&item| tree.items.parent(item) == Some(key));

                let update = match existing {
                    // Clean items keep their entire subtree.
                    Some(item) if !force && !self.force && tree.items[item].flags.is_empty() => {
                        Update {
                            key: item,
                            descend: false,
                            force: false,
                        }
                    }

                    _ => self.update(ui, tree, child, existing, &parent_style, cx),
                };

                if next == Some(update.key) {
                    next = tree.items.next_sibling(update.key);
                } else {
                    if let Some(next) = next {
                        tree.items.before(next, update.key);
                    } else {
                        tree.items.append(key, update.key);
                    }

                    changed = true;
                }

                if existing.is_none() {
                    // Propagate the new item's flags to the ancestors.
                    tree.mark_dirty(update.key);
                }

                if update.descend {
                    self.pending.push(Task {
                        key: update.key,
                        node: child,
                        force: update.force,
                    });
                }
            }
        }

        while let Some(item) = next {
            next = tree.items.next_sibling(item);
            tree.delete_item(item);
            changed = true;
        }

        if changed {
            tree.mark_dirty(key);
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
    ) -> Update {
        let (kind, child_layout) = classify(ui, node);
        let text_style = conv::resolve_text_style(ui, node, cx, parent_style);

        let Some(key) = existing else {
            let key = tree.add_item(LayoutInputItem {
                kind,
                node,
                text_style,
                child_layout,
                flags: LayoutFlags::all(),
            });

            return Update {
                key,
                descend: true,
                force: false,
            };
        };

        let item = &mut tree.items[key];

        if self.force {
            item.kind = kind;
            item.text_style = text_style;
            item.child_layout = child_layout;
            item.flags = LayoutFlags::all();

            return Update {
                key,
                descend: true,
                force: true,
            };
        }

        let style_changed =
            !Arc::ptr_eq(&item.text_style, &text_style) && item.text_style != text_style;
        let changed = style_changed || item.kind != kind || item.child_layout != child_layout;
        let descend = changed || !item.flags.is_empty();

        // Always take the new allocation so equal styles keep sharing it.
        item.text_style = text_style;
        item.kind = kind;
        item.child_layout = child_layout;

        if changed {
            tree.mark_dirty(key);
        }

        Update {
            key,
            descend,
            force: style_changed,
        }
    }

    fn clear(&mut self) {
        self.pending.clear();
        self.force = false;
    }
}

struct Task {
    key: LayoutInputKey,
    node: NodeKey,
    /// The text style changed, so descendants must be resolved again.
    force: bool,
}

struct Update {
    key: LayoutInputKey,
    descend: bool,
    force: bool,
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
