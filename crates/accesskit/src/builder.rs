use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
};
use comet_paint::prepaint::tree::PrepaintTree;
use comet_tree::ui::Ui;
use kurbo::Affine;
use parley::PositionedLayoutItem;

use crate::{
    A11yFlags,
    style::Role,
    tree::{A11yNode, A11yTree, node_id},
};

/// Layout flags of an item whose fragments are resolved again:
/// - [`DirtyFlags::NEED_LAYER`]: a paint property of the item changed.
/// - [`DirtyFlags::NEED_CHILDREN`]: its children changed. It includes
///   [`DirtyFlags::NEED_LAYER`].
/// - [`DirtyFlags::NEED_PRE_PAINT`]: its fragments were laid out, moved or
///   resized, or their transforms changed, so their bounds or transforms may
///   have changed.
const RESOLVE: DirtyFlags = DirtyFlags::NEED_LAYER.union(DirtyFlags::NEED_PRE_PAINT);

/// Builds and updates an [`A11yTree`] for a layout. Keep it to reuse its
/// buffers across builds.
#[derive(Default)]
pub struct A11yTreeBuilder {
    /// Items to visit, with their accessibility parent item and whether it
    /// was resolved.
    pending: Vec<Visit>,
    /// Fragments of the item being visited.
    fragments: Vec<FragmentKey>,
    /// Parents of new fragments that were not resolved themselves, resolved
    /// at the end for their new children, with their item and its parent.
    parents: Vec<(FragmentKey, LayoutInputKey, Option<LayoutInputKey>)>,
}

#[derive(Clone, Copy)]
struct Visit {
    item: LayoutInputKey,
    /// Nearest ancestor item with fragments, whose fragments hold the nodes
    /// of this item's fragments.
    parent: Option<LayoutInputKey>,
    /// Whether the fragments of `parent` were resolved in this build.
    parent_resolved: bool,
}

/// What resolving reads from.
#[derive(Clone, Copy)]
struct Sources<'a> {
    ui: &'a Ui,
    inputs: &'a LayoutInputTree,
    fragments: &'a FragmentTree,
    prepaint: &'a PrepaintTree,
}

impl A11yTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build into `tree` the nodes of the items under the layout input item
    /// `root` that need it, adding the fragments whose nodes changed to the
    /// tree's [changed](A11yTree::changed) list, sent and cleared by
    /// [`A11yTree::take_update`].
    ///
    /// The tree follows the layout input tree, in the order of the UI: each
    /// item has a node for each of its [fragments](FragmentTree::fragments),
    /// its box or its inline and glyph run fragments on every line, and the
    /// nodes of its child items are children of its own nodes. Line boxes
    /// and anonymous blocks have no nodes. Items without fragments are left
    /// out, their children taking their place.
    ///
    /// A child item's fragment is a child of the parent item's fragment
    /// containing it in the fragment tree, such as the span of an inline
    /// element on the same line. A fragment not contained in any, such as a
    /// block splitting an inline element, is a child of the parent item's
    /// first fragment.
    ///
    /// Each node has its fragment's role, its border box as bounds, the
    /// transform mapping it into its parent node, from the pre-paint
    /// transforms, and the nodes of its child items' fragments as children.
    /// Glyph runs are labelled with their text.
    ///
    /// The fragments of an item are resolved:
    /// - when its flags in `dirty` have [`DirtyFlags::NEED_LAYER`], which
    ///   [`DirtyFlags::NEED_CHILDREN`] includes, or
    ///   [`DirtyFlags::NEED_PRE_PAINT`],
    /// - when it is [`A11yFlags::DIRTY`] in `tree`,
    /// - when a fragment has no node, and then so is the parent fragment,
    ///   for its new children,
    /// - when its parent item was resolved and a fragment is not contained
    ///   in the parent's fragments, as its transform depends on them.
    ///
    /// Items are visited from the root, going into the children of an item
    /// only when its flags or child bits in `dirty` have one of those layout
    /// flags, when it is [`A11yFlags::CHILD_DIRTY`], or when one of its
    /// fragments has no node. Resolved nodes equal to their previous node are
    /// not added to the changed list.
    ///
    /// When the root item's box fragment has no node in `tree`, such as on
    /// the first build or after a new root, every node is resolved and other
    /// nodes are dropped, so the next update sends the whole tree.
    ///
    /// Build after resolving `prepaint` for `fragments`, and before clearing
    /// `dirty`. Returns `None` if `root` has no box fragment, or if an item
    /// no longer matches the UI or a fragment has no pre-paint data. Nodes
    /// resolved before the failure stay in `tree`, without their flags
    /// cleared, so the next build resolves them again; build from a
    /// [cleared](A11yTree::clear) tree to send a complete tree again.
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        dirty: &DirtyMap,
        fragments: &FragmentTree,
        prepaint: &PrepaintTree,
        tree: &mut A11yTree,
        root: LayoutInputKey,
    ) -> Option<()> {
        let root_fragment = fragments.box_fragment(root)?;
        let sources = Sources {
            ui,
            inputs,
            fragments,
            prepaint,
        };
        let Self {
            pending,
            fragments: own,
            parents,
        } = self;
        pending.clear();
        parents.clear();

        // A root without a node, new or never built, resolves every node.
        if !tree.nodes.contains_key(root_fragment) {
            tree.clear();
        } else {
            // Fragments changed by earlier builds and deleted since are no
            // longer in the tree.
            tree.changed
                .retain(|&key| fragments.items.get(key).is_some());
        }

        pending.push(Visit {
            item: root,
            parent: None,
            parent_resolved: true,
        });

        while let Some(Visit {
            item,
            parent,
            parent_resolved,
        }) = pending.pop()
        {
            inputs.items.get(item)?;
            let entry = dirty.entry(item);
            let flags = tree.flags.get(item).copied().unwrap_or_default();

            own.clear();
            own.extend(fragments.fragments(item));
            let missing = own.iter().any(|&key| !tree.nodes.contains_key(key));

            let resolve = missing
                || flags.contains(A11yFlags::DIRTY)
                || entry.flags.intersects(RESOLVE)
                || (parent_resolved
                    && parent.is_some_and(|parent| {
                        own.iter()
                            .any(|&key| sources.contained(key, parent).is_none())
                    }));

            for &key in own.iter() {
                // The parent lists its new child only once resolved again.
                if !tree.nodes.contains_key(key)
                    && !parent_resolved
                    && let Some(parent) = parent
                {
                    let parent_key = sources.parent_fragment(key, parent)?;
                    let grandparent = sources.parent_item(parent);
                    parents.push((parent_key, parent, grandparent));
                }

                if resolve {
                    sources.resolve(tree, key, item, parent)?;
                }
            }

            if missing
                || flags.contains(A11yFlags::CHILD_DIRTY)
                || (entry.flags | entry.child_bits).intersects(RESOLVE)
            {
                // Items without fragments leave their children to the parent.
                let (parent, parent_resolved) = if own.is_empty() {
                    (parent, parent_resolved)
                } else {
                    (Some(item), resolve)
                };
                // Popped in order.
                let start = pending.len();
                pending.extend(inputs.items.children(item).map(|item| Visit {
                    item,
                    parent,
                    parent_resolved,
                }));
                pending[start..].reverse();
            }

            tree.flags.remove(item);
        }

        parents.sort_unstable_by_key(|&(key, ..)| key);
        parents.dedup_by_key(|&mut (key, ..)| key);
        for &(key, item, parent) in parents.iter() {
            sources.resolve(tree, key, item, parent)?;
        }

        Some(())
    }
}

impl Sources<'_> {
    /// Resolve the node of `key`, a fragment of `item`, into `tree`, adding
    /// it to the changed list when it differs from its previous node.
    /// `parent` is the item whose fragments hold it.
    fn resolve(
        &self,
        tree: &mut A11yTree,
        key: FragmentKey,
        item: LayoutInputKey,
        parent: Option<LayoutInputKey>,
    ) -> Option<()> {
        let node = self.resolve_node(key, item, parent)?;

        match tree.nodes.get_mut(key) {
            Some(old) if old.node == node => {}
            Some(old) => {
                old.node = node;
                tree.changed.push(key);
            }
            None => {
                tree.nodes.insert(key, A11yNode { node });
                tree.changed.push(key);
            }
        }

        Some(())
    }

    /// Resolve the node of one fragment from the UI, its layout and its
    /// pre-paint data.
    fn resolve_node(
        &self,
        key: FragmentKey,
        item: LayoutInputKey,
        parent: Option<LayoutInputKey>,
    ) -> Option<accesskit::Node> {
        let fragment = self.fragments.items.get(key)?;
        let props = self.ui.props(self.inputs.items.get(item)?.node)?;

        let role = match fragment.item {
            FragmentItem::GlyphRun { .. } => accesskit::Role::TextRun,
            _ => {
                let Role(role) = props.get_cloned().unwrap_or_default();
                role
            }
        };

        let mut node = accesskit::Node::new(role);
        let size = fragment.layout.size;
        node.set_bounds(accesskit::Rect::new(0.0, 0.0, size.width, size.height));

        let transform = match parent {
            Some(parent) => self.relative_transform(key, self.parent_fragment(key, parent)?)?,
            // The root has no parent fragment.
            None => self.prepaint.fragment(key)?.transform,
        };
        if transform != Affine::IDENTITY {
            node.set_transform(accesskit::Affine::new(transform.as_coeffs()));
        }

        let mut children = Vec::new();
        self.children(key, item, item, &mut children);
        node.set_children(children);

        if let FragmentItem::GlyphRun { line, item } = fragment.item
            && let Some(text) = self.glyph_run_text(key, line, item)
        {
            node.set_label(text);
        }

        Some(node)
    }

    /// Add to `children` the nodes of the fragments of the child items of
    /// `items`, in order, whose parent fragment is `key`, a fragment of
    /// `item`. Children without fragments are replaced by their children.
    fn children(
        &self,
        key: FragmentKey,
        item: LayoutInputKey,
        items: LayoutInputKey,
        children: &mut Vec<accesskit::NodeId>,
    ) {
        for child in self.inputs.items.children(items) {
            let mut fragments = self.fragments.fragments(child).peekable();
            if fragments.peek().is_none() {
                self.children(key, item, child, children);
                continue;
            }

            children.extend(
                fragments
                    .filter(|&child| self.parent_fragment(child, item) == Some(key))
                    .map(node_id),
            );
        }
    }

    /// The nearest ancestor item of `item` with fragments.
    fn parent_item(&self, item: LayoutInputKey) -> Option<LayoutInputKey> {
        let items = &self.inputs.items;
        core::iter::successors(items.parent(item), |&item| items.parent(item))
            .find(|&item| self.fragments.fragments(item).next().is_some())
    }

    /// The fragment of `parent` holding the node of `key`: the one containing
    /// it, or else its first fragment.
    fn parent_fragment(&self, key: FragmentKey, parent: LayoutInputKey) -> Option<FragmentKey> {
        self.contained(key, parent)
            .or_else(|| self.fragments.fragments(parent).next())
    }

    /// The nearest ancestor of `key` with a node, if it is a fragment of
    /// `parent`. Line boxes and anonymous blocks have no nodes.
    fn contained(&self, key: FragmentKey, parent: LayoutInputKey) -> Option<FragmentKey> {
        let items = &self.fragments.items;
        let ancestor =
            core::iter::successors(items.parent(key), |&key| items.parent(key)).find(|&key| {
                !matches!(
                    items[key].item,
                    FragmentItem::Line { .. } | FragmentItem::AnonymousBlock { .. }
                )
            })?;

        (items[ancestor].source == parent).then_some(ancestor)
    }

    /// The transform mapping `key` into `parent`. Without `parent` among
    /// the ancestors of `key`, it goes through the root fragment.
    fn relative_transform(&self, key: FragmentKey, parent: FragmentKey) -> Option<Affine> {
        let items = &self.fragments.items;
        let mut transform = Affine::IDENTITY;
        let mut current = Some(key);

        // Usually an ancestor: no inverse is needed.
        while let Some(key) = current {
            if key == parent {
                return Some(transform);
            }
            transform = self.prepaint.fragment(key)?.transform * transform;
            current = items.parent(key);
        }

        Some(self.root_transform(parent)?.inverse() * transform)
    }

    /// The transform mapping `key` into the space of the root fragment's
    /// parent.
    fn root_transform(&self, key: FragmentKey) -> Option<Affine> {
        let items = &self.fragments.items;
        core::iter::successors(Some(key), |&key| items.parent(key))
            .try_fold(Affine::IDENTITY, |transform, key| {
                Some(self.prepaint.fragment(key)?.transform * transform)
            })
    }

    /// The text of a glyph run fragment, from the inline layout of the line
    /// containing it.
    fn glyph_run_text(&self, key: FragmentKey, line: usize, item: usize) -> Option<&str> {
        let items = &self.fragments.items;
        let line_key = core::iter::successors(items.parent(key), |&key| items.parent(key))
            .find(|&key| matches!(items[key].item, FragmentItem::Line { .. }))?;
        let inline = self.fragments.inline_layout(line_key)?;
        let line = inline.layout.get(line)?;
        let Some(PositionedLayoutItem::GlyphRun(run)) = line.items().nth(item) else {
            return None;
        };

        // The shaping run may hold the text of other items, whose styles
        // differ: only the clusters of the glyph run's style are its text.
        let style = run.style_index();
        let range = run
            .run()
            .clusters()
            .filter(|cluster| cluster.style_index() == style)
            .map(|cluster| cluster.text_range())
            .reduce(|range, cluster| range.start.min(cluster.start)..range.end.max(cluster.end))?;

        inline.text.get(range)
    }
}
