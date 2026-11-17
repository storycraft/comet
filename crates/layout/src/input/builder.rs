use std::sync::Arc;

use comet_tree::{
    node::{Node, NodeKey},
    ui::{Props, Ui},
};

use crate::{
    input::{
        ChildLayout, InputFlags, LayoutFlags, LayoutInputItem, LayoutInputKey, LayoutInputKind,
        TextStyle,
        text::{resolve_text_style, root_text_style},
        tree::LayoutInputTree,
    },
    style::{
        bx::{DisplayInner, DisplayOuter},
        geometry::{Size, UnitResolveContext},
    },
};

/// What [`LayoutInputTreeBuilder::build`] builds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildRequest {
    /// Source node to build. Its existing item in the tree is updated; without
    /// one, a new tree is built.
    pub source: NodeKey,
    /// Font size of the root element, used for `rem` and as the initial font size.
    pub root_font_size: f64,
    /// Viewport size, used for `vw` and `vh`.
    pub viewport: Size<f64>,
}

impl BuildRequest {
    /// Request a build of `source`.
    pub const fn new(source: NodeKey, root_font_size: f64, viewport: Size<f64>) -> Self {
        Self {
            source,
            root_font_size,
            viewport,
        }
    }
}

/// Builds a [`LayoutInputTree`] that mirrors the source UI 1:1.
#[derive(Default)]
pub struct LayoutInputTreeBuilder {
    pending: Vec<LayoutInputKey>,
}

impl LayoutInputTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a new tree for `request.source` when the node has no item, or
    /// update its existing item with the source changes marked with
    /// [`LayoutInputTree::mark`]. Only subtrees whose
    /// [`LayoutInputItem::child_flags`] have [`InputFlags::NEED_CHILDREN`]
    /// are visited. For each visited item, by its [`LayoutInputItem::flags2`]:
    ///
    /// - [`InputFlags::NEED_FULL_LAYOUT`] resolves the item again, including
    ///   its text style, and marks every child with it, so the style
    ///   propagates to the whole subtree,
    /// - [`InputFlags::NEED_CHILDREN`], which the former includes, reconciles
    ///   its children with the source children, deleting the stale ones and
    ///   adding new ones marked [`InputFlags::NEED_FULL_LAYOUT`].
    ///
    /// Items whose text styles depend on the viewport are not resolved again
    /// when it changes; invalidate them with
    /// [`LayoutInputTree::invalidate_viewport`] first. Box styles are not
    /// resolved here, so the fragment builder handles boxes depending on the
    /// viewport.
    ///
    /// Keys are reused, and unmarked source changes are not seen. To insert a
    /// new node under an existing item, mark its parent
    /// [`InputFlags::NEED_CHILDREN`].
    ///
    /// Returns the item generated for `source`. Returns `None` and deletes its item
    /// if `source` is not in the UI. Also returns `None` without changes when
    /// `source` has a UI parent but its item has no parent item, since the tree
    /// no longer mirrors the UI. Build from the UI root first.
    ///
    /// Flags are left for later steps, and cleared with
    /// [`LayoutInputTree::clear_flags`] once consumed. Use `item_for_node`
    /// for child results.
    ///
    /// The root font size of an existing tree is not tracked:
    /// [clear](LayoutInputTree::clear) the tree after it changes.
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
        } = request;
        let target = tree.item_for_node(source);

        let Some(props) = ui.props(source) else {
            if let Some(key) = target {
                let parent = tree.items.parent(key);

                tree.delete_item(key);

                if let Some(parent) = parent {
                    tree.mark(parent, InputFlags::NEED_CHILDREN);
                }
            }

            return None;
        };

        // A source with a UI parent always has a parent item, so the tree
        // does not mirror the UI.
        if target.and_then(|key| tree.items.parent(key)).is_none() && props.parent().is_some() {
            return None;
        }

        let key = match target {
            Some(key) => key,
            None => {
                let style = Arc::new(root_text_style(root_font_size));
                let key = tree.add_item(new_item(source, props, style));

                tree.mark(key, InputFlags::NEED_FULL_LAYOUT);
                key
            }
        };

        // Nothing under a clean item changed.
        if !needs_visit(&tree.items[key]) {
            return Some(key);
        }

        let cx = UnitResolveContext {
            root_font_size,
            font_size: root_font_size,
            viewport,
        };

        self.pending.clear();
        self.pending.push(key);

        while let Some(key) = self.pending.pop() {
            self.visit(ui, tree, key, &cx);
        }

        Some(key)
    }

    /// Resolve the item and reconcile its children as its flags require, then
    /// queue the children to visit.
    fn visit(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        key: LayoutInputKey,
        cx: &UnitResolveContext,
    ) {
        let node = tree.items[key].node;
        let Some(props) = ui.props(node) else {
            // The source node was deleted without marking its parent.
            tree.delete_item(key);
            return;
        };
        let flags = tree.items[key].flags2;
        let full = flags.contains(InputFlags::NEED_FULL_LAYOUT);

        if full {
            resolve(tree, key, props, cx);
        }

        if flags.contains(InputFlags::NEED_CHILDREN) {
            reconcile_children(ui, tree, key, node);
        }

        let mut next = tree.items.first_child(key);

        while let Some(child) = next {
            next = tree.items.next_sibling(child);

            // Children inherit the resolved style.
            if full {
                tree.mark(child, InputFlags::NEED_FULL_LAYOUT);
            }

            if needs_visit(&tree.items[child]) {
                self.pending.push(child);
            }
        }
    }
}

/// Whether the item or its subtree has something to build.
fn needs_visit(item: &LayoutInputItem) -> bool {
    // `NEED_FULL_LAYOUT` includes `NEED_CHILDREN`.
    item.flags2.contains(InputFlags::NEED_CHILDREN)
        || item.child_flags.contains(InputFlags::NEED_CHILDREN)
}

/// A detached item for `node` with `text_style`, which a visit with
/// [`InputFlags::NEED_FULL_LAYOUT`] resolves.
fn new_item(node: NodeKey, props: Props, text_style: Arc<TextStyle>) -> LayoutInputItem {
    let (kind, child_layout) = classify(props.kind(), props);

    LayoutInputItem {
        child_flags: InputFlags::empty(),
        flags2: InputFlags::empty(),
        kind,
        node,
        text_style,
        child_layout,
        flags: LayoutFlags::empty(),
    }
}

/// Resolve the source props of an item: its kind, child layout and the text
/// style inherited from its parent item.
fn resolve(tree: &mut LayoutInputTree, key: LayoutInputKey, props: Props, cx: &UnitResolveContext) {
    let parent_style = match tree.items.parent(key) {
        Some(parent) => tree.items[parent].text_style.clone(),
        None => Arc::new(root_text_style(cx.root_font_size)),
    };
    let (kind, child_layout) = classify(props.kind(), props);
    let text_style = resolve_text_style(props, cx, &parent_style);

    let item = &mut tree.items[key];
    item.kind = kind;
    item.child_layout = child_layout;
    // Always take the new allocation so equal styles keep sharing it.
    tree.set_text_style(key, text_style);
}

/// Match the item's children to the source children, in order. Stale
/// children are deleted, and new ones are added marked
/// [`InputFlags::NEED_FULL_LAYOUT`].
fn reconcile_children(ui: &Ui, tree: &mut LayoutInputTree, key: LayoutInputKey, node: NodeKey) {
    let mut next = tree.items.first_child(key);

    if tree.items[key].child_layout != ChildLayout::None {
        let parent_style = tree.items[key].text_style.clone();

        for child in ui.children(node) {
            let Some(props) = ui.props(child) else {
                continue;
            };
            let existing = tree
                .item_for_node(child)
                .filter(|&item| tree.items.parent(item) == Some(key));
            let item = match existing {
                Some(item) => item,
                None => tree.add_item(new_item(child, props, parent_style.clone())),
            };

            // Items whose nodes left this parent are deleted first, so
            // the children after them are not taken as moved.
            while let Some(stale) = next
                && stale != item
                && ui
                    .props(tree.items[stale].node)
                    .and_then(|props| props.parent())
                    != Some(node)
            {
                next = tree.items.next_sibling(stale);
                tree.delete_item(stale);
            }

            if next == Some(item) {
                next = tree.items.next_sibling(item);
            } else if let Some(next) = next {
                tree.items.before(next, item);
            } else {
                tree.items.append(key, item);
            }

            if existing.is_none() {
                // Once attached, so the flags reach the ancestors.
                tree.mark(item, InputFlags::NEED_FULL_LAYOUT);
            }
        }
    }

    while let Some(item) = next {
        next = tree.items.next_sibling(item);
        tree.delete_item(item);
    }
}

/// Derive the item kind and child layout from the node's own display.
/// Splitting inline content around blocks is left to later stages.
fn classify(node: Node, props: Props) -> (LayoutInputKind, ChildLayout) {
    if node == Node::Text {
        return (LayoutInputKind::Text, ChildLayout::None);
    }

    let outer = props.get_or_default::<DisplayOuter>();
    let inner = props.get::<DisplayInner>();
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
