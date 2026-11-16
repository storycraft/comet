use std::sync::Arc;

use comet_tree::{
    node::{Node, NodeKey},
    ui::{Props, Ui},
};

use crate::{
    input::{
        ChildLayout, LayoutFlags, LayoutInputItem, LayoutInputKey, LayoutInputKind, TextStyle,
        text::{TextStyleBits, resolve_text_style, root_text_style},
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
    pending: Vec<Task>,
    /// Items to visit in [`Self::update_viewport`].
    visit: Vec<LayoutInputKey>,
    /// Source nodes of the subtrees [`Self::update_viewport`] builds again.
    viewport_roots: Vec<NodeKey>,
}

impl LayoutInputTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the item generated for `request.source`, or build a new tree for it
    /// when the node has no item. The whole subtree of `source` is resolved
    /// again to reconcile children and text styles. Keys are reused. To insert a new node under an existing item, update its
    /// parent.
    ///
    /// Returns the item generated for `source`. Returns `None` and deletes its item
    /// if `source` is not in the UI. Also returns `None` without changes when
    /// `source` has a UI parent but its item has no parent item, since the tree
    /// no longer mirrors the UI. Build from the UI root first.
    ///
    /// Generated items have every [`LayoutFlags`] set. Existing items are
    /// marked [`LayoutFlags::NEED_FULL_LAYOUT`] when their resolved text style, kind
    /// or child list changed. Mark changes the builder cannot see, such as
    /// edited text or box styles, as usual. Flags are
    /// left for the consumer of the input tree to clear. Use `item_for_node`
    /// for child results.
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
                    tree.mark(parent, LayoutFlags::NEED_FULL_LAYOUT);
                }
            }

            return None;
        };

        let parent_style = match target.and_then(|key| tree.items.parent(key)) {
            Some(parent) => tree.items[parent].text_style.clone(),
            // A source with a UI parent always has a parent item, so the tree
            // does not mirror the UI.
            None if props.parent().is_some() => return None,
            None => Arc::new(root_text_style(root_font_size)),
        };

        self.pending.clear();

        let cx = UnitResolveContext {
            root_font_size,
            font_size: root_font_size,
            viewport,
        };

        let key = self.update(ui, tree, source, target, &parent_style, &cx)?;

        self.pending.push(Task { key, node: source });

        while let Some(task) = self.pending.pop() {
            self.reconcile_children(ui, tree, task, &cx);
        }

        Some(key)
    }

    /// Build again the subtrees under the item of `request.source` whose text
    /// styles depend on the viewport, after the viewport changed. Starting
    /// from that item, each topmost item with
    /// [`TextStyleBits::DEPENDS_VIEWPORT`] is built with `request` for its own
    /// node, as [`Self::build`]; its descendants inherit the bit and are built
    /// with it. Other items are kept as they are.
    ///
    /// Box styles are not resolved here, so items whose box styles use `vw`
    /// or `vh` must be marked separately.
    ///
    /// Returns the item of `request.source`, or `None` if it has no item.
    /// Nothing is visited when no item in the tree depends on the viewport.
    pub fn update_viewport(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        request: BuildRequest,
    ) -> Option<LayoutInputKey> {
        let source = tree.item_for_node(request.source)?;

        if tree.viewport_dependent_count() == 0 {
            return Some(source);
        }

        let mut visit = std::mem::take(&mut self.visit);
        let mut roots = std::mem::take(&mut self.viewport_roots);
        visit.clear();
        roots.clear();
        visit.push(source);

        // Collect the subtree roots first, since building them changes the tree.
        while let Some(key) = visit.pop() {
            let item = &tree.items[key];

            if item
                .text_style
                .bits
                .contains(TextStyleBits::DEPENDS_VIEWPORT)
            {
                roots.push(item.node);
            } else {
                visit.extend(tree.items.children(key));
            }
        }

        for &node in &roots {
            self.build(
                ui,
                tree,
                BuildRequest {
                    source: node,
                    ..request
                },
            );
        }

        self.visit = visit;
        self.viewport_roots = roots;

        // Building a subtree may delete it, but never the source item above it.
        tree.item_for_node(request.source)
    }

    /// Match the item's children to the source children, in order.
    fn reconcile_children(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        Task { key, node }: Task,
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

                let Some(item) = self.update(ui, tree, child, existing, &parent_style, cx) else {
                    continue;
                };

                if next == Some(item) {
                    next = tree.items.next_sibling(item);
                } else {
                    if let Some(next) = next {
                        tree.items.before(next, item);
                    } else {
                        tree.items.append(key, item);
                    }

                    changed = true;
                }

                if existing.is_none() {
                    // Propagate the new item's flags to the ancestors.
                    tree.mark(item, LayoutFlags::NEED_FULL_LAYOUT);
                }

                self.pending.push(Task {
                    key: item,
                    node: child,
                });
            }
        }

        while let Some(item) = next {
            next = tree.items.next_sibling(item);
            tree.delete_item(item);
            changed = true;
        }

        if changed {
            tree.mark(key, LayoutFlags::NEED_FULL_LAYOUT);
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
    ) -> Option<LayoutInputKey> {
        let props = ui.props(node)?;
        let (kind, child_layout) = classify(props.kind(), props);
        let text_style = resolve_text_style(props, cx, parent_style);

        let Some(key) = existing else {
            return Some(tree.add_item(LayoutInputItem {
                kind,
                node,
                text_style,
                child_layout,
                flags: LayoutFlags::all(),
            }));
        };

        let item = &mut tree.items[key];
        let changed = (!Arc::ptr_eq(&item.text_style, &text_style)
            && item.text_style != text_style)
            || item.kind != kind
            || item.child_layout != child_layout;

        item.kind = kind;
        item.child_layout = child_layout;
        // Always take the new allocation so equal styles keep sharing it.
        tree.replace_text_style(key, text_style);

        // Content is only refragmented when the resolved item changed.
        if changed {
            tree.mark(key, LayoutFlags::NEED_FULL_LAYOUT);
        }

        Some(key)
    }
}

struct Task {
    key: LayoutInputKey,
    node: NodeKey,
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
