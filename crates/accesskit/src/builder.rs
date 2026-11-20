use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{FragmentItem, FragmentKey, tree::FragmentTree},
    input::{LayoutInputKey, tree::LayoutInputTree},
};
use comet_paint::prepaint::tree::PrepaintTree;
use comet_tree::ui::Ui;
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

/// Builds and updates an [`A11yTree`] for a fragment tree. Keep it to reuse
/// its buffers across builds.
#[derive(Default)]
pub struct A11yTreeBuilder {
    /// Fragments to visit, with whether their parent was resolved.
    pending: Vec<(FragmentKey, bool)>,
    /// Parents of new fragments that were not resolved themselves, resolved
    /// at the end for their new children.
    parents: Vec<FragmentKey>,
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

    /// Build into `tree` the nodes of the fragments under the box fragment of
    /// the layout input item `root` that need it, adding the fragments whose
    /// nodes changed to the tree's [changed](A11yTree::changed) list, sent and
    /// cleared by [`A11yTree::take_update`].
    ///
    /// Each node has its fragment's role, its border box as bounds, its
    /// pre-paint transform, which maps it into its parent fragment, and the
    /// nodes of its child fragments as children. Glyph runs are labelled with
    /// their text.
    ///
    /// Fragments are resolved:
    /// - when their item's flags in `dirty` have [`DirtyFlags::NEED_LAYER`],
    ///   which [`DirtyFlags::NEED_CHILDREN`] includes, or
    ///   [`DirtyFlags::NEED_PRE_PAINT`],
    /// - when they are [`A11yFlags::DIRTY`] in `tree`,
    /// - when they have no node, and then so is their parent, for its new
    ///   children.
    ///
    /// Fragments are visited from the root, going into the children of a
    /// fragment only when its item's flags or child bits have one of those
    /// layout flags, when it is [`A11yFlags::CHILD_DIRTY`], or when it has no
    /// node. Lines and anonymous blocks carry the item of the box owning
    /// them, so they are resolved with it. Resolved nodes equal to their
    /// previous node are not added to the changed list.
    ///
    /// When the root fragment has no node in `tree`, such as on the first
    /// build or after a new root, every node is resolved and nodes of other
    /// fragments are dropped, so the next update sends the whole tree.
    ///
    /// Build after resolving `prepaint` for `fragments`, and before clearing
    /// `dirty`. Returns `None` if `root` has no box fragment, or if a
    /// fragment no longer matches the UI or has no pre-paint data. Nodes
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
        let Self { pending, parents } = self;
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

        pending.push((root_fragment, true));

        while let Some((key, parent_resolved)) = pending.pop() {
            let fragment = fragments.items.get(key)?;
            inputs.items.get(fragment.source)?;
            let entry = dirty.entry(fragment.source);
            let flags = tree.nodes.get(key).map(|node| node.flags);
            let missing = flags.is_none();
            let flags = flags.unwrap_or_default();

            let resolve =
                missing || flags.contains(A11yFlags::DIRTY) || entry.flags.intersects(RESOLVE);
            if resolve {
                sources.resolve(tree, key)?;
            }

            // The parent lists its new child only once resolved again.
            if missing
                && !parent_resolved
                && let Some(parent) = fragments.items.parent(key)
            {
                parents.push(parent);
            }

            if missing
                || flags.contains(A11yFlags::CHILD_DIRTY)
                || (entry.flags | entry.child_bits).intersects(RESOLVE)
            {
                pending.extend(fragments.items.children(key).map(|child| (child, resolve)));
            }

            if let Some(node) = tree.nodes.get_mut(key) {
                node.flags = A11yFlags::empty();
            }
        }

        parents.sort_unstable();
        parents.dedup();
        for &key in parents.iter() {
            sources.resolve(tree, key)?;
        }

        Some(())
    }
}

impl Sources<'_> {
    /// Resolve the node of a fragment into `tree`, adding it to the changed
    /// list when it differs from its previous node. Its flags are kept.
    fn resolve(&self, tree: &mut A11yTree, key: FragmentKey) -> Option<()> {
        let node = self.resolve_node(key)?;

        match tree.nodes.get_mut(key) {
            Some(old) if old.node == node => {}
            Some(old) => {
                old.node = node;
                tree.changed.push(key);
            }
            None => {
                tree.nodes.insert(
                    key,
                    A11yNode {
                        node,
                        flags: A11yFlags::empty(),
                    },
                );
                tree.changed.push(key);
            }
        }

        Some(())
    }

    /// Resolve the node of one fragment from the UI, its layout and its
    /// pre-paint data.
    fn resolve_node(&self, key: FragmentKey) -> Option<accesskit::Node> {
        let fragment = self.fragments.items.get(key)?;
        let props = self
            .ui
            .props(self.inputs.items.get(fragment.source)?.node)?;
        let data = self.prepaint.fragment(key)?;

        // Lines and anonymous blocks belong to the item owning them, but are
        // not its own fragments.
        let own = !matches!(
            fragment.item,
            FragmentItem::Line { .. } | FragmentItem::AnonymousBlock { .. }
        );
        let role = match fragment.item {
            FragmentItem::GlyphRun { .. } => accesskit::Role::TextRun,
            _ => {
                let Role(role) = own
                    .then(|| props.get_cloned())
                    .flatten()
                    .unwrap_or_default();
                role
            }
        };

        let mut node = accesskit::Node::new(role);
        let size = fragment.layout.size;
        node.set_bounds(accesskit::Rect::new(0.0, 0.0, size.width, size.height));
        if data.transform != Default::default() {
            node.set_transform(accesskit::Affine::new(data.transform.as_coeffs()));
        }
        node.set_children(
            self.fragments
                .items
                .children(key)
                .map(node_id)
                .collect::<Vec<_>>(),
        );

        if let FragmentItem::GlyphRun { line, item } = fragment.item
            && let Some(text) = self.glyph_run_text(key, line, item)
        {
            node.set_label(text);
        }

        Some(node)
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

        inline.text.get(run.run().text_range())
    }
}
