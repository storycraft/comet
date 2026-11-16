use std::sync::Arc;

use comet_tree::{
    node::{Node, NodeKey},
    ui::{Props, Ui},
};

use crate::{
    input::{
        ChildLayout, InputFlags, LayoutFlags, LayoutInputItem, LayoutInputKey, LayoutInputKind,
        TextStyle,
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
    /// Whether the viewport differs from the one the existing tree was built
    /// with. Items whose text styles depend on it are then resolved again.
    pub viewport_changed: bool,
}

impl BuildRequest {
    /// Request a build of `source`, with an unchanged viewport.
    pub const fn new(source: NodeKey, root_font_size: f64, viewport: Size<f64>) -> Self {
        Self {
            source,
            root_font_size,
            viewport,
            viewport_changed: false,
        }
    }

    /// Set [`Self::viewport_changed`].
    pub const fn with_viewport_changed(mut self, viewport_changed: bool) -> Self {
        self.viewport_changed = viewport_changed;
        self
    }
}

/// Text style bits leading a build after a viewport change to the items
/// resolving differently.
const VIEWPORT_BITS: TextStyleBits =
    TextStyleBits::DEPENDS_VIEWPORT.union(TextStyleBits::CHILD_DEPENDS_VIEWPORT);

/// Builds a [`LayoutInputTree`] that mirrors the source UI 1:1.
#[derive(Default)]
pub struct LayoutInputTreeBuilder {
    pending: Vec<Task>,
    /// [`BuildRequest::viewport_changed`] of the current build.
    viewport_changed: bool,
}

impl LayoutInputTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a new tree for `request.source` when the node has no item, or
    /// update its existing item with the source changes marked with
    /// [`InputFlags`], visiting only marked items:
    ///
    /// - items marked [`InputFlags::NEED_STYLE`] are resolved again, and so
    ///   are their descendants while their text styles keep changing,
    /// - items marked [`InputFlags::NEED_CHILDREN`], or whose child layout
    ///   changed, reconcile their children, building new ones,
    /// - subtrees without [`InputFlags::CHILD_NEED_INPUT`] are skipped.
    ///
    /// With [`BuildRequest::viewport_changed`], items whose text styles
    /// depend on the viewport ([`TextStyleBits::DEPENDS_VIEWPORT`]) are
    /// resolved again as if marked [`InputFlags::NEED_STYLE`], found through
    /// [`TextStyleBits::CHILD_DEPENDS_VIEWPORT`]. Box styles are not resolved
    /// here, so the fragment builder handles boxes depending on the viewport.
    ///
    /// Keys are reused. Mark changes with
    /// [`LayoutInputTree::mark_input`] before building; unmarked source
    /// changes are not seen. To insert a new node under an existing item,
    /// mark its parent [`InputFlags::NEED_CHILDREN`].
    ///
    /// Returns the item generated for `source`. Returns `None` and deletes its item
    /// if `source` is not in the UI. Also returns `None` without changes when
    /// `source` has a UI parent but its item has no parent item, since the tree
    /// no longer mirrors the UI. Build from the UI root first.
    ///
    /// Generated items have every [`LayoutFlags`] set. Existing items are
    /// marked [`LayoutFlags::NEED_FULL_LAYOUT`] when their resolved text style, kind
    /// or child list changed, and children moved among their siblings are
    /// marked [`LayoutFlags::MOVED`]. Mark changes the builder cannot see, such as
    /// edited text or box styles, as usual. Flags are
    /// left for the consumer of the input tree to clear. Use `item_for_node`
    /// for child results.
    ///
    /// The input flags of visited items are cleared. Ancestors of the item of
    /// `source` keep their [`InputFlags::CHILD_NEED_INPUT`], so build from the
    /// root unless other subtrees are built separately.
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
            viewport_changed,
        } = request;
        let target = tree.item_for_node(source);

        let Some(props) = ui.props(source) else {
            if let Some(key) = target {
                let parent = tree.items.parent(key);

                tree.delete_item(key);

                if let Some(parent) = parent {
                    tree.mark_child_list(parent);
                }
            }

            return None;
        };

        self.viewport_changed = viewport_changed;

        // Nothing under a clean item changed.
        if let Some(key) = target
            && !self.needs_visit(&tree.items[key])
        {
            return Some(key);
        }

        let (parent_style, parent_bits) = match target.and_then(|key| tree.items.parent(key)) {
            Some(parent) => (
                tree.items[parent].text_style.clone(),
                tree.items[parent].bits,
            ),
            // A source with a UI parent always has a parent item, so the tree
            // does not mirror the UI.
            None if props.parent().is_some() => return None,
            None => (
                Arc::new(root_text_style(root_font_size)),
                TextStyleBits::empty(),
            ),
        };

        self.pending.clear();

        let cx = UnitResolveContext {
            root_font_size,
            font_size: root_font_size,
            viewport,
        };

        let (key, visit) = match target {
            Some(key) if !self.needs_resolve(&tree.items[key]) => {
                (key, Visit::update(Change::NONE))
            }
            _ => {
                let (key, change) =
                    Self::resolve(ui, tree, source, target, (&parent_style, parent_bits), &cx)?;
                let visit = match target {
                    Some(_) => Visit::update(change),
                    // A new tree is built entirely.
                    None => Visit::Full,
                };

                (key, visit)
            }
        };

        if tree.items[key]
            .bits
            .contains(TextStyleBits::DEPENDS_VIEWPORT)
        {
            tree.propagate_viewport(key);
        }

        self.pending.push(Task {
            key,
            node: source,
            visit,
        });

        while let Some(task) = self.pending.pop() {
            self.visit_children(ui, tree, task, &cx);
        }

        Some(key)
    }

    /// Whether an existing item must be resolved again, unless its parent
    /// restyles it anyway.
    fn needs_resolve(&self, item: &LayoutInputItem) -> bool {
        item.input_flags.contains(InputFlags::NEED_STYLE)
            || (self.viewport_changed && item.bits.contains(TextStyleBits::DEPENDS_VIEWPORT))
    }

    /// Whether an existing item or its subtree has something to build.
    fn needs_visit(&self, item: &LayoutInputItem) -> bool {
        !item.input_flags.is_empty()
            || (self.viewport_changed && item.bits.intersects(VIEWPORT_BITS))
    }

    /// Consume the input flags of a resolved item and resolve its children
    /// as `task` requires, queueing those to visit.
    fn visit_children(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        Task { key, node, visit }: Task,
        cx: &UnitResolveContext,
    ) {
        let flags = std::mem::take(&mut tree.items[key].input_flags);
        let (restyle, reconcile) = match visit {
            Visit::Full => (true, true),
            Visit::Update { restyle, reconcile } => (
                restyle,
                reconcile || flags.contains(InputFlags::NEED_CHILDREN),
            ),
        };

        if reconcile {
            self.reconcile_children(ui, tree, key, node, visit, cx);
        } else {
            self.update_children(ui, tree, key, restyle, cx);
        }
    }

    /// Match the item's children to the source children, in order. Existing
    /// children are resolved again when `visit` restyles them or they are
    /// marked [`InputFlags::NEED_STYLE`]; new children are built.
    fn reconcile_children(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        key: LayoutInputKey,
        node: NodeKey,
        visit: Visit,
        cx: &UnitResolveContext,
    ) {
        let mut next = tree.items.first_child(key);
        let mut changed = false;

        if tree.items[key].child_layout != ChildLayout::None {
            let parent_style = tree.items[key].text_style.clone();
            let parent_bits = tree.items[key].bits;

            for child in ui.children(node) {
                let existing = tree
                    .item_for_node(child)
                    .filter(|&item| tree.items.parent(item) == Some(key));

                let Some((item, child_visit)) = self.resolve_child(
                    ui,
                    tree,
                    child,
                    existing,
                    visit,
                    (&parent_style, parent_bits),
                    cx,
                ) else {
                    continue;
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
                    changed = true;
                }

                if next == Some(item) {
                    next = tree.items.next_sibling(item);
                } else {
                    if let Some(next) = next {
                        tree.items.before(next, item);
                    } else {
                        tree.items.append(key, item);
                    }

                    changed = true;

                    if existing.is_some() {
                        tree.mark(item, LayoutFlags::MOVED);
                    }
                }

                if existing.is_none() {
                    // Propagate the new item's flags to the ancestors.
                    tree.mark(item, LayoutFlags::NEED_FULL_LAYOUT);
                }

                if let Some(visit) = child_visit {
                    // Once attached, so the bit reaches its ancestors.
                    propagate_viewport(tree, item);
                    self.pending.push(Task {
                        key: item,
                        node: child,
                        visit,
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
            tree.mark_child_list(key);
        }
    }

    /// Resolve the item's existing children that `restyle` or their input
    /// flags require, keeping the child list.
    fn update_children(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        key: LayoutInputKey,
        restyle: bool,
        cx: &UnitResolveContext,
    ) {
        let parent_style = tree.items[key].text_style.clone();
        let parent_bits = tree.items[key].bits;
        let visit = Visit::Update {
            restyle,
            reconcile: false,
        };
        let mut next = tree.items.first_child(key);
        let mut changed = false;

        while let Some(item) = next {
            next = tree.items.next_sibling(item);
            let child = tree.items[item].node;

            match self.resolve_child(
                ui,
                tree,
                child,
                Some(item),
                visit,
                (&parent_style, parent_bits),
                cx,
            ) {
                Some((_, Some(visit))) => {
                    propagate_viewport(tree, item);
                    self.pending.push(Task {
                        key: item,
                        node: child,
                        visit,
                    });
                }
                Some((_, None)) => {}
                // The source node was deleted without marking its parent.
                None => {
                    tree.delete_item(item);
                    changed = true;
                }
            }
        }

        if changed {
            tree.mark_child_list(key);
        }
    }

    /// Resolve a child of an item visited with `visit`, if needed, and decide
    /// how to visit it. Returns the child's item, with `None` when its
    /// subtree needs no visit, or `None` if `node` is not in the UI.
    #[allow(clippy::too_many_arguments)]
    fn resolve_child(
        &mut self,
        ui: &Ui,
        tree: &mut LayoutInputTree,
        node: NodeKey,
        existing: Option<LayoutInputKey>,
        visit: Visit,
        parent: (&Arc<TextStyle>, TextStyleBits),
        cx: &UnitResolveContext,
    ) -> Option<(LayoutInputKey, Option<Visit>)> {
        let restyle = match visit {
            Visit::Full => true,
            Visit::Update { restyle, .. } => restyle,
        };
        let marked = existing.is_some_and(|item| self.needs_resolve(&tree.items[item]));

        let Some(item) = existing.filter(|_| !restyle && !marked) else {
            let (item, change) = Self::resolve(ui, tree, node, existing, parent, cx)?;
            let visit = match visit {
                // New subtrees are built entirely.
                _ if existing.is_none() => Visit::Full,
                Visit::Full => Visit::Full,
                Visit::Update { .. } => Visit::update(change),
            };

            return Some((item, Some(visit)));
        };

        // Unchanged children are only visited for their marked descendants,
        // or descendants depending on a changed viewport.
        ui.props(node)?;

        let visit = self
            .needs_visit(&tree.items[item])
            .then_some(Visit::update(Change::NONE));

        Some((item, visit))
    }

    /// Resolve the source node and write it into `existing` or a new item.
    /// Returns the item with what changed in it, or `None` if `node` is not in
    /// the UI.
    fn resolve(
        ui: &Ui,
        tree: &mut LayoutInputTree,
        node: NodeKey,
        existing: Option<LayoutInputKey>,
        (parent_style, parent_bits): (&Arc<TextStyle>, TextStyleBits),
        cx: &UnitResolveContext,
    ) -> Option<(LayoutInputKey, Change)> {
        let props = ui.props(node)?;
        let (kind, child_layout) = classify(props.kind(), props);
        let (text_style, bits) = resolve_text_style(props, cx, parent_style, parent_bits);

        let Some(key) = existing else {
            let key = tree.add_item(LayoutInputItem {
                kind,
                node,
                text_style,
                bits,
                child_layout,
                flags: LayoutFlags::all(),
                input_flags: InputFlags::empty(),
            });

            return Some((key, Change::ALL));
        };

        let item = &mut tree.items[key];
        let style_changed =
            !Arc::ptr_eq(&item.text_style, &text_style) && item.text_style != text_style;
        let change = Change {
            // Children inherit the style and its viewport dependence.
            style: style_changed || (item.bits ^ bits).contains(TextStyleBits::DEPENDS_VIEWPORT),
            child_layout: item.child_layout != child_layout,
        };
        let kind_changed = item.kind != kind;
        let changed = style_changed || kind_changed || change.child_layout;

        item.kind = kind;
        item.child_layout = child_layout;
        // Always take the new allocation so equal styles keep sharing it.
        item.text_style = text_style;
        // Descendants keep their marks; they are only ever added.
        item.bits = bits | (item.bits & TextStyleBits::CHILD_DEPENDS_VIEWPORT);
        // The item is resolved.
        item.input_flags.remove(InputFlags::NEED_STYLE);

        // Content is only refragmented when the resolved item changed.
        if changed {
            tree.mark(key, LayoutFlags::NEED_FULL_LAYOUT);
        }

        // The box owning the item's content lays it out by its kind.
        if kind_changed {
            tree.mark_content_owner(key);
        }

        Some((key, change))
    }
}

/// What resolving an existing item changed, for its children.
#[derive(Clone, Copy)]
struct Change {
    /// Its text style or viewport dependence, inherited by its children.
    style: bool,
    /// Its child layout, deciding whether it has children.
    child_layout: bool,
}

impl Change {
    const NONE: Self = Self {
        style: false,
        child_layout: false,
    };
    const ALL: Self = Self {
        style: true,
        child_layout: true,
    };
}

/// How the children of a resolved item are visited.
#[derive(Clone, Copy)]
enum Visit {
    /// Resolve and reconcile the whole subtree.
    Full,
    /// Visit only what changed and what input flags mark.
    Update {
        /// The item's inherited style changed, so its children are resolved.
        restyle: bool,
        /// Its children are reconciled with the source children.
        reconcile: bool,
    },
}

impl Visit {
    const fn update(change: Change) -> Self {
        Self::Update {
            restyle: change.style,
            reconcile: change.child_layout,
        }
    }
}

struct Task {
    key: LayoutInputKey,
    node: NodeKey,
    visit: Visit,
}

/// Mark the ancestors of a resolved, attached item whose text style depends
/// on the viewport.
fn propagate_viewport(tree: &mut LayoutInputTree, key: LayoutInputKey) {
    if tree.items[key]
        .bits
        .contains(TextStyleBits::DEPENDS_VIEWPORT)
    {
        tree.propagate_viewport(key);
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
