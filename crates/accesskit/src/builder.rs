use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
};
use comet_paint::prepaint::tree::PrepaintTree;
use comet_tree::{node::Node, ui::Ui};
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
    /// Fragments of the item being visited, with the item whose fragments
    /// hold their nodes.
    fragments: Vec<(FragmentKey, Option<LayoutInputKey>)>,
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
    /// Glyph runs carry their text as a [`TextRun`](accesskit::Role::TextRun):
    /// its value and the metrics of its characters. Platform adapters never
    /// expose text runs themselves, only through an ancestor whose text they
    /// make up, such as a [`Label`](accesskit::Role::Label).
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

            // Its linked fragments, then the glyph runs inside its own box,
            // held by the box.
            own.clear();
            own.extend(fragments.fragments(item).map(|key| (key, parent)));
            if let Some(&(first, _)) = own.first() {
                own.extend(sources.own_runs(item, first).map(|key| (key, Some(item))));
            }
            let missing = own.iter().any(|&(key, _)| !tree.nodes.contains_key(key));

            let resolve = missing
                || flags.contains(A11yFlags::DIRTY)
                || entry.flags.intersects(RESOLVE)
                || (parent_resolved
                    && parent.is_some_and(|parent| {
                        own.iter().any(|&(key, holder)| {
                            holder == Some(parent) && sources.contained(key, parent).is_none()
                        })
                    }));

            for &(key, holder) in own.iter() {
                // The parent lists its new child only once resolved again.
                // A new run in the item's own box resolves the item.
                if !tree.nodes.contains_key(key)
                    && !parent_resolved
                    && holder == parent
                    && let Some(parent) = parent
                {
                    let parent_key = sources.parent_fragment(key, parent)?;
                    let grandparent = sources.parent_item(parent);
                    parents.push((parent_key, parent, grandparent));
                }

                if resolve {
                    sources.resolve(tree, key, item, holder)?;
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
        let node = self.inputs.items.get(item)?.node;
        let props = self.ui.props(node)?;

        // Glyph runs are text runs whatever the node's role. Without a role,
        // texts are labels and boxes generic containers.
        let role = match fragment.item {
            FragmentItem::GlyphRun { .. } => accesskit::Role::TextRun,
            _ => match props.get_cloned() {
                Some(Role(role)) => role,
                None if self.ui.kind(node) == Some(Node::Text) => accesskit::Role::Label,
                None => accesskit::Role::GenericContainer,
            },
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

        let mut children: Vec<_> = self.own_runs(item, key).map(node_id).collect();
        self.children(key, item, item, &mut children);
        node.set_children(children);

        if let FragmentItem::GlyphRun { line, item } = fragment.item {
            self.text_run(&mut node, key, line, item);
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

    /// The glyph runs of `item` inside `key`, when it is the item's own box,
    /// such as the box of a text in a flex container. They are not linked
    /// with the box.
    fn own_runs(
        &self,
        item: LayoutInputKey,
        key: FragmentKey,
    ) -> impl Iterator<Item = FragmentKey> {
        let items = &self.fragments.items;
        // Only texts own glyph runs, which saves walking other boxes.
        let own_box = self
            .inputs
            .items
            .get(item)
            .is_some_and(|input| input.kind == LayoutInputKind::Text)
            && items
                .get(key)
                .is_some_and(|fragment| fragment.item.is_box() && fragment.source == item);

        own_box
            .then(|| items.descendants(key))
            .into_iter()
            .flatten()
            .filter(move |&key| {
                matches!(items[key].item, FragmentItem::GlyphRun { .. })
                    && items[key].source == item
            })
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

    /// Set the text of a glyph run fragment on its node, from the inline
    /// layout of the line containing it, as `accesskit` expects of a
    /// [`TextRun`](accesskit::Role::TextRun): its value, the length of each
    /// character, their positions and widths in the text direction, the
    /// characters starting words, and the text runs before and after it on
    /// the same line.
    ///
    /// A character is a cluster: the smallest unit of text the layout
    /// places. Text runs are only exposed through an ancestor whose text
    /// they make up, such as a [`Label`](accesskit::Role::Label).
    fn text_run(&self, node: &mut accesskit::Node, key: FragmentKey, line: usize, item: usize) {
        let items = &self.fragments.items;
        let Some(line_key) = core::iter::successors(items.parent(key), |&key| items.parent(key))
            .find(|&key| matches!(items[key].item, FragmentItem::Line { .. }))
        else {
            return;
        };
        let Some(inline) = self.fragments.inline_layout(line_key) else {
            return;
        };
        let Some(PositionedLayoutItem::GlyphRun(run)) = inline
            .layout
            .get(line)
            .and_then(|line| line.items().nth(item))
        else {
            return;
        };

        // The shaping run may hold the text of other items, whose styles
        // differ: only the clusters of the glyph run's style are its text,
        // in logical order.
        let style = run.style_index();
        let mut range: Option<core::ops::Range<usize>> = None;
        let mut lengths = Vec::new();
        let mut positions = Vec::new();
        let mut widths = Vec::new();
        let mut word_starts = Vec::new();
        let mut position = 0.0;
        for cluster in run
            .run()
            .clusters()
            .filter(|cluster| cluster.style_index() == style)
        {
            let text = cluster.text_range();
            // Word starts are bytes in `accesskit`: later ones are left out.
            if cluster.is_word_boundary()
                && let Ok(index) = u8::try_from(lengths.len())
            {
                word_starts.push(index);
            }
            // A run has one direction, so the first character in logical
            // order is the first in the text direction.
            lengths.push(u8::try_from(text.len()).unwrap_or(u8::MAX));
            positions.push(position);
            widths.push(cluster.advance());
            position += cluster.advance();
            range = Some(match range {
                Some(range) => range.start.min(text.start)..range.end.max(text.end),
                None => text,
            });
        }
        let Some(value) = range.and_then(|range| inline.text.get(range)) else {
            return;
        };

        node.set_value(value);
        node.set_character_lengths(lengths);
        node.set_character_positions(positions);
        node.set_character_widths(widths);
        node.set_word_starts(word_starts);
        node.set_text_direction(if run.run().is_rtl() {
            accesskit::TextDirection::RightToLeft
        } else {
            accesskit::TextDirection::LeftToRight
        });

        // Glyph runs of a line are in visual order.
        let runs: Vec<_> = items
            .descendants(line_key)
            .filter(|&key| matches!(items[key].item, FragmentItem::GlyphRun { .. }))
            .collect();
        if let Some(index) = runs.iter().position(|&run| run == key) {
            if let Some(&previous) = index.checked_sub(1).and_then(|index| runs.get(index)) {
                node.set_previous_on_line(node_id(previous));
            }
            if let Some(&next) = runs.get(index + 1) {
                node.set_next_on_line(node_id(next));
            }
        }
    }
}
