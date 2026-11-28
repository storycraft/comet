use std::sync::Arc;

use comet_tree::{
    node::{Node, NodeKey},
    ui::{Props, Ui},
};

use crate::{
    dirty_map::{DirtyEntry, DirtyFlags, DirtyMap},
    input::{
        ChildLayout, LayoutInputItem, LayoutInputKey, LayoutInputKind, TextStyle,
        text::{resolve_text_style, root_text_style},
        tree::LayoutInputTree,
    },
    style::bx::{DisplayInner, DisplayOuter},
};
use comet_geometry::{Size, UnitResolveContext};

/// Builds a [`LayoutInputTree`] that mirrors the source UI 1:1.
#[derive(Default)]
pub struct LayoutInputTreeBuilder {
    /// Existing items to visit.
    pending: Vec<LayoutInputKey>,
    /// Items of a new subtree whose children are not built yet, with their
    /// source nodes.
    subtree: Vec<(LayoutInputKey, NodeKey)>,
}

impl LayoutInputTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve the item and reconcile its children as its flags require,
    /// queueing the children to visit.
    fn visit(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        dirty: &mut DirtyMap,
        key: LayoutInputKey,
        cx: &UnitResolveContext,
    ) {
        let node = tree.items[key].node;
        let Some(props) = ui.props(node) else {
            // The source node was deleted without marking its parent.
            delete_child(tree, dirty, key);
            return;
        };
        let flags = dirty.entry(key).flags;
        let full = flags.contains(DirtyFlags::NEED_FULL_LAYOUT);
        // Whether every child is marked `NEED_FULL_LAYOUT`: children inherit
        // the resolved style, and whether they generate boxes depends on the
        // child layout. Children of an item resolving as before keep theirs.
        let mut cascade = false;

        if full {
            let parent_style = match tree.items.parent(key) {
                Some(parent) => tree.items[parent].text_style.clone(),
                None => Arc::new(root_text_style(cx.root_font_size)),
            };
            let (kind, child_layout, text_style) = resolve(props, &parent_style, cx);

            let item = &mut tree.items[key];
            let kind_changed = item.kind != kind;
            // Pre-paint resolves `em` lengths with the font size; the rest
            // of what it reads comes from props or fragments.
            let font_size_changed = item.text_style.span.font_size != text_style.span.font_size;
            cascade = item.child_layout != child_layout
                || !(Arc::ptr_eq(&item.text_style, &text_style) || item.text_style == text_style);
            item.kind = kind;
            item.child_layout = child_layout;
            // Always take the new allocation so equal styles keep sharing it.
            tree.set_text_style(key, text_style);

            if kind_changed {
                mark_content_owner(tree, dirty, key);
            }

            if font_size_changed {
                dirty.mark(tree, key, DirtyFlags::NEED_PRE_PAINT);
            }
        }

        // Subtrees are visited only for descendants needing it, or to
        // cascade a changed style to every child.
        let descend = cascade
            || dirty
                .entry(key)
                .child_bits
                .contains(DirtyFlags::NEED_CHILDREN);

        // Children are checked against the source children only when the
        // child list is marked. `NEED_FULL_LAYOUT` includes `NEED_CHILDREN`.
        if flags.contains(DirtyFlags::NEED_CHILDREN) {
            // The children of an inline element are inline content of its
            // owner, whose mode depends on their kinds.
            if tree.items[key].kind == LayoutInputKind::Inline {
                mark_content_owner(tree, dirty, key);
            }

            self.reconcile_children(ui, tree, dirty, key, node, cascade, descend, cx);
            return;
        }

        if !descend {
            return;
        }

        let mut next = tree.items.first_child(key);

        while let Some(child) = next {
            next = tree.items.next_sibling(child);
            self.queue(tree, dirty, child, false);
        }
    }

    /// Match the item's children to the source children, in order. Stale
    /// children are deleted, and the subtrees of new ones are built and
    /// attached. With `full`, kept children are marked
    /// [`DirtyFlags::NEED_FULL_LAYOUT`] to inherit the changed style; with
    /// `descend`, those needing a visit are queued.
    #[allow(clippy::too_many_arguments)]
    fn reconcile_children(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        dirty: &mut DirtyMap,
        key: LayoutInputKey,
        node: NodeKey,
        full: bool,
        descend: bool,
        cx: &UnitResolveContext,
    ) {
        let mut next = tree.items.first_child(key);

        if tree.items[key].child_layout != ChildLayout::None {
            let parent_style = tree.items[key].text_style.clone();

            for child in ui.children(node) {
                let existing = tree
                    .item_for_node(child)
                    .filter(|&item| tree.items.parent(item) == Some(key));
                let item = match existing {
                    Some(item) => item,
                    None => {
                        let Some(item) =
                            self.build_subtree(ui, tree, dirty, child, &parent_style, cx)
                        else {
                            continue;
                        };

                        item
                    }
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
                    delete_child(tree, dirty, stale);
                }

                if next == Some(item) {
                    next = tree.items.next_sibling(item);
                } else if let Some(next) = next {
                    tree.items.before(next, item);
                } else {
                    tree.items.append(key, item);
                }

                match existing {
                    Some(item) if descend => self.queue(tree, dirty, item, full),
                    Some(_) => {}
                    // Its descendants are marked already. Marking the
                    // attached root marks its new ancestors too.
                    None => dirty.mark(tree, item, DirtyFlags::NEED_FULL_LAYOUT),
                }
            }
        }

        while let Some(item) = next {
            next = tree.items.next_sibling(item);
            delete_child(tree, dirty, item);
        }
    }

    /// Mark an existing child [`DirtyFlags::NEED_FULL_LAYOUT`] with `full`,
    /// and queue it if its subtree has something to build.
    fn queue(
        &mut self,
        tree: &LayoutInputTree,
        dirty: &mut DirtyMap,
        key: LayoutInputKey,
        full: bool,
    ) {
        if full {
            dirty.mark(tree, key, DirtyFlags::NEED_FULL_LAYOUT);
        }

        if needs_visit(dirty.entry(key)) {
            self.pending.push(key);
        }
    }

    /// Build the whole subtree of `node` as a detached tree, its root
    /// inheriting `parent_style`. Every descendant is marked
    /// [`DirtyFlags::NEED_FULL_LAYOUT`]; the root is left for the caller to
    /// mark once attached, so the mark reaches its ancestors. Returns its
    /// root, or `None` if `node` is not in the UI.
    fn build_subtree(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        dirty: &mut DirtyMap,
        node: NodeKey,
        parent_style: &Arc<TextStyle>,
        cx: &UnitResolveContext,
    ) -> Option<LayoutInputKey> {
        let root = tree.add_item(new_item(node, ui.props(node)?, parent_style, cx));

        self.subtree.clear();
        self.subtree.push((root, node));

        while let Some((key, node)) = self.subtree.pop() {
            if tree.items[key].child_layout == ChildLayout::None {
                continue;
            }

            let style = tree.items[key].text_style.clone();

            for child in ui.children(node) {
                let Some(props) = ui.props(child) else {
                    continue;
                };
                let item = tree.add_item(new_item(child, props, &style, cx));

                tree.items.append(key, item);
                dirty.mark(tree, item, DirtyFlags::NEED_FULL_LAYOUT);
                self.subtree.push((item, child));
            }
        }

        Some(root)
    }
}

#[bon::bon]
impl LayoutInputTreeBuilder {
    /// Build a new tree for `source` when the node has no item, or
    /// update its existing item with the source changes marked in `dirty`
    /// with [`DirtyMap::mark`]. Only subtrees whose
    /// [`DirtyEntry::child_bits`] have [`DirtyFlags::NEED_CHILDREN`]
    /// are visited. For each visited item, by its [`DirtyEntry::flags`]:
    ///
    /// - [`DirtyFlags::NEED_FULL_LAYOUT`] resolves the item again, including
    ///   its text style. When its text style or child layout changed, every
    ///   child is marked with it, so the style propagates down until items
    ///   resolve as before,
    /// - [`DirtyFlags::NEED_CHILDREN`], which the former includes, reconciles
    ///   its children with the source children, deleting the stale ones and
    ///   marking the item [`DirtyFlags::NEED_FULL_LAYOUT`] when any is gone.
    ///   The whole subtree of each new child is built at once, every item
    ///   marked [`DirtyFlags::NEED_FULL_LAYOUT`], and attached.
    ///
    /// Items whose text styles depend on the viewport are not resolved again
    /// when it changes; invalidate them with
    /// [`LayoutInputTree::invalidate_viewport`] first. Box styles are not
    /// resolved here, so the fragment builder handles boxes depending on the
    /// viewport.
    ///
    /// Keys are reused, and unmarked source changes are not seen. To insert a
    /// new node under an existing item, mark its parent
    /// [`DirtyFlags::NEED_CHILDREN`].
    ///
    /// Returns the item generated for `source`. Returns `None` and deletes its item
    /// if `source` is not in the UI, marking its parent
    /// [`DirtyFlags::NEED_FULL_LAYOUT`]. Also returns `None` without changes when
    /// `source` has a UI parent but its item has no parent item, since the tree
    /// no longer mirrors the UI. Build from the UI root first.
    ///
    /// Flags are left in `dirty` for later steps, and cleared with
    /// [`DirtyMap::clear`] once consumed. Use `item_for_node`
    /// for child results.
    ///
    /// The root font size of an existing tree is not tracked:
    /// [clear](LayoutInputTree::clear) the tree after it changes.
    #[builder(finish_fn = build)]
    pub fn prepare(
        &mut self,
        #[builder(start_fn)] tree: &mut LayoutInputTree,
        #[builder(start_fn)] dirty: &mut DirtyMap,
        ui: &Ui,
        /// Source node to build. Its existing item in the tree is updated;
        /// without one, a new tree is built.
        source: NodeKey,
        /// Font size of the root element, used for `rem` and as the initial
        /// font size.
        root_font_size: f64,
        /// Viewport size, used for `vw` and `vh`.
        viewport: Size<f64>,
    ) -> Option<LayoutInputKey> {
        let target = tree.item_for_node(source);

        let Some(props) = ui.props(source) else {
            if let Some(key) = target {
                delete_child(tree, dirty, key);
            }

            return None;
        };

        // A source with a UI parent always has a parent item, so the tree
        // does not mirror the UI.
        if target.and_then(|key| tree.items.parent(key)).is_none() && props.parent().is_some() {
            return None;
        }

        let cx = UnitResolveContext {
            root_font_size,
            font_size: root_font_size,
            viewport,
        };

        // Without an item, the source is built like a new child.
        let Some(key) = target else {
            let style = Arc::new(root_text_style(root_font_size));

            let key = self.build_subtree(ui, tree, dirty, source, &style, &cx)?;
            dirty.mark(tree, key, DirtyFlags::NEED_FULL_LAYOUT);

            return Some(key);
        };

        // Nothing under a clean item changed.
        if !needs_visit(dirty.entry(key)) {
            return Some(key);
        }

        self.pending.clear();
        self.pending.push(key);

        while let Some(key) = self.pending.pop() {
            self.visit(ui, tree, dirty, key, &cx);
        }

        Some(key)
    }
}

/// Delete an item and its subtree, and mark its parent
/// [`DirtyFlags::NEED_FULL_LAYOUT`]: its child list changed, so later steps
/// lay it out again. The builder resolved the parent already, so the mark
/// does not restyle its children in this build.
fn delete_child(tree: &mut LayoutInputTree, dirty: &mut DirtyMap, key: LayoutInputKey) {
    let parent = tree.items.parent(key);

    tree.delete_item(key);

    if let Some(parent) = parent {
        dirty.mark(tree, parent, DirtyFlags::NEED_FULL_LAYOUT);
    }
}

/// Mark [`DirtyFlags::NEED_CHILDREN`] the nearest ancestor of an item that is
/// not an inline element: the box whose children, or whose inline content,
/// include the item. Its layout mode depends on the kinds of those items, so
/// mark it when an item changes kind, or when an inline element's child list
/// changes. The builder visited the ancestor already, so the mark is only
/// read by later steps.
fn mark_content_owner(tree: &LayoutInputTree, dirty: &mut DirtyMap, key: LayoutInputKey) {
    let mut owner = tree.items.parent(key);

    while let Some(key) = owner
        && tree.items[key].kind == LayoutInputKind::Inline
    {
        owner = tree.items.parent(key);
    }

    if let Some(owner) = owner {
        dirty.mark(tree, owner, DirtyFlags::NEED_CHILDREN);
    }
}

/// Whether the item or its subtree has something to build.
fn needs_visit(entry: DirtyEntry) -> bool {
    // `NEED_FULL_LAYOUT` includes `NEED_CHILDREN`.
    entry.flags.contains(DirtyFlags::NEED_CHILDREN)
        || entry.child_bits.contains(DirtyFlags::NEED_CHILDREN)
}

/// Resolve the source props of an item: its kind, child layout and the text
/// style inherited from `parent_style`.
fn resolve(
    props: Props,
    parent_style: &Arc<TextStyle>,
    cx: &UnitResolveContext,
) -> (LayoutInputKind, ChildLayout, Arc<TextStyle>) {
    let (kind, child_layout) = classify(props.kind(), props);

    (
        kind,
        child_layout,
        resolve_text_style(props, cx, parent_style),
    )
}

/// A resolved, detached item for `node`.
fn new_item(
    node: NodeKey,
    props: Props,
    parent_style: &Arc<TextStyle>,
    cx: &UnitResolveContext,
) -> LayoutInputItem {
    let (kind, child_layout, text_style) = resolve(props, parent_style, cx);

    LayoutInputItem {
        kind,
        node,
        text_style,
        child_layout,
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
        Some(DisplayInner::Content(_)) => ChildLayout::None,
        Some(DisplayInner::Container(_)) => ChildLayout::Container,
        // Inline flow children participate in the surrounding inline formatting context.
        _ if kind == LayoutInputKind::Inline => ChildLayout::Inline,
        _ => ChildLayout::Block,
    };

    (kind, child_layout)
}
