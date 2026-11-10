use std::sync::Arc;

use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};

use crate::{
    input::{
        ChildLayout, LayoutFlags, LayoutInputItem, LayoutInputKey, LayoutInputKind, TextStyle,
        text::{resolve_text_style, root_text_style},
        tree::LayoutInputTree,
    },
    style::{
        bx::{DisplayInner, DisplayOuter},
        geometry::{Size, UnitResolveContext},
    },
};

/// What [`LayoutInputTreeBuilder::build`] builds, and how.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildRequest {
    /// Source node to build. Its existing item in the tree is updated; without
    /// one, a new tree is built.
    pub source: NodeKey,
    /// Font size of the root element, used for `rem` and as the initial font size.
    pub root_font_size: f64,
    /// Viewport size, used for `vw` and `vh`.
    pub viewport: Size<f64>,
    /// Re-resolve the whole subtree of `source` and flag every item as if it were
    /// newly generated, even when its item is clean. Keys are still reused.
    /// Use it when the root font size or the viewport changes, since every item
    /// may depend on them.
    pub force: bool,
}

impl BuildRequest {
    /// Request a fresh build of `source`.
    pub const fn new(source: NodeKey, root_font_size: f64, viewport: Size<f64>) -> Self {
        Self {
            source,
            root_font_size,
            viewport,
            force: false,
        }
    }

    pub const fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }
}

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

    /// Update the item generated for `request.source`, or build a new tree for it
    /// when the node has no item. Clean items return immediately unless forced;
    /// mark changed nodes before updating. To insert a new node under an existing
    /// item, update its parent.
    ///
    /// Returns the item generated for `source`. Returns `None` and deletes its item
    /// if `source` is not in the UI, even when the item is clean. Also returns
    /// `None` without changes when `source` has a UI parent but its item has no
    /// parent item, since the tree no longer mirrors the UI. Build from the UI
    /// root first.
    ///
    /// Generated items have every [`LayoutFlags`] set. Updating visits flagged
    /// items and keeps clean subtrees as they are, unless an inherited text style
    /// changed. Existing items are only marked dirty when their contents or child
    /// lists change. Flags are left for the consumer of the input tree to clear.
    /// Items only describe their own node, so any changed node can be updated on
    /// its own. See [`BuildRequest::force`] for forced builds. Use
    /// `item_for_node` for child results.
    pub fn build(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        request: BuildRequest,
    ) -> Option<LayoutInputKey> {
        let BuildRequest {
            source,
            root_font_size,
            viewport,
            force,
        } = request;
        let target = tree.item_for_node(source);

        if ui.kind(source).is_none() {
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

        let parent_style = match target.and_then(|key| tree.items.parent(key)) {
            Some(parent) => tree.items[parent].text_style.clone(),
            // A source with a UI parent always has a parent item, so the tree
            // does not mirror the UI.
            None if ui.parent(source).is_some() => return None,
            None => Arc::new(root_text_style(root_font_size)),
        };

        self.clear();
        self.force = force;

        let cx = UnitResolveContext {
            root_font_size,
            font_size: root_font_size,
            viewport,
        };

        let update = self.update(ui, tree, source, target, &parent_style, &cx);

        if force {
            // Propagate the reset flags to the ancestors.
            tree.mark_dirty(update.key);
        }

        if update.descend {
            self.pending.push(Task {
                key: update.key,
                node: source,
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
        let text_style = resolve_text_style(ui, node, cx, parent_style);

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
