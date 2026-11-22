use accesskit::NodeId;
use comet_layout::input::{LayoutInputKey, tree::LayoutInputTree};
use comet_slotmap_tree::SlotTree;
use comet_tree::node::NodeKey;
use slotmap::SecondaryMap;

use crate::{AxFlags, AxKey, AxNode, AxNodeKind, node_id};

/// Accessibility nodes of a layout, following the layout input tree in the
/// order of the UI: each item has one node, holding the nodes of its child
/// items, and a text item also holds a [`TextRun`](accesskit::Role::TextRun)
/// node for each of its glyph runs, before them.
///
/// Nodes are kept in a [`SlotTree`] mirroring the accessibility tree, and
/// their keys are their [`NodeId`]s, so they keep their ids as long as their
/// items exist. Text run nodes are reused in order when the text is laid out
/// again. Built and updated by an
/// [`AxTreeBuilder`](crate::AxTreeBuilder).
#[derive(Debug, Default)]
pub struct AxTree {
    pub(crate) nodes: SlotTree<AxKey, AxNode>,
    /// Node of each layout input item. Entries of deleted nodes are stale.
    pub(crate) items: SecondaryMap<LayoutInputKey, AxKey>,
    /// Accessibility changes of the layout input items, cleared by the
    /// builder once visited.
    pub(crate) flags: SecondaryMap<LayoutInputKey, AxFlags>,
    /// Nodes changed since the last update.
    pub(crate) changed: Vec<AxKey>,
}

impl AxTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Nodes changed since the last [update](Self::take_update), possibly
    /// more than once each, and possibly deleted since.
    pub fn changed(&self) -> &[AxKey] {
        &self.changed
    }

    /// The nodes changed since the last update, for the nodes of an
    /// [`accesskit::TreeUpdate`], and clear the [changed](Self::changed)
    /// list. After a build from a new root, they are every node of the tree.
    pub fn take_update(&mut self) -> Vec<(NodeId, accesskit::Node)> {
        let mut changed = core::mem::take(&mut self.changed);
        changed.sort_unstable();
        changed.dedup();

        let nodes = changed
            .iter()
            .filter_map(|&key| Some((node_id(key), self.nodes.get(key)?.node.clone())))
            .collect();
        // Keep the buffer.
        changed.clear();
        self.changed = changed;

        nodes
    }

    /// The nodes, in a tree mirroring the accessibility tree.
    pub fn nodes(&self) -> &SlotTree<AxKey, AxNode> {
        &self.nodes
    }

    pub fn get(&self, key: AxKey) -> Option<&AxNode> {
        self.nodes.get(key)
    }

    /// The node of a layout input item.
    pub fn item_node(&self, item: LayoutInputKey) -> Option<AxKey> {
        self.items
            .get(item)
            .copied()
            .filter(|&key| self.nodes.get(key).is_some_and(|node| node.item == item))
    }

    /// Accessibility changes of a layout input item.
    pub fn flags(&self, key: LayoutInputKey) -> AxFlags {
        self.flags.get(key).copied().unwrap_or_default()
    }

    /// Mark a layout input item [`AxFlags::DIRTY`], and its ancestors
    /// [`AxFlags::CHILD_DIRTY`] up to the root item, so the next build
    /// resolves its node again.
    pub fn mark(&mut self, inputs: &LayoutInputTree, key: LayoutInputKey) {
        self.insert_flags(key, AxFlags::DIRTY);

        let mut ancestor = inputs.items.parent(key);
        while let Some(key) = ancestor {
            // Ancestors of a marked item are marked already.
            if self.flags(key).contains(AxFlags::CHILD_DIRTY) {
                break;
            }
            self.insert_flags(key, AxFlags::CHILD_DIRTY);
            ancestor = inputs.items.parent(key);
        }
    }

    /// [Mark](Self::mark) the layout input item of a source node. Nodes not
    /// laid out are ignored.
    pub fn mark_node(&mut self, inputs: &LayoutInputTree, node: NodeKey) {
        if let Some(item) = inputs.item_for_node(node) {
            self.mark(inputs, item);
        }
    }

    fn insert_flags(&mut self, key: LayoutInputKey, flags: AxFlags) {
        match self.flags.get_mut(key) {
            Some(current) => current.insert(flags),
            None => {
                self.flags.insert(key, flags);
            }
        }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.items.clear();
        self.flags.clear();
        self.changed.clear();
    }

    /// The node of `item`, inserted unresolved if it has none.
    pub(crate) fn item_node_or_insert(&mut self, item: LayoutInputKey) -> AxKey {
        if let Some(key) = self.item_node(item) {
            return key;
        }

        let key = self.nodes.insert(AxNode {
            node: accesskit::Node::default(),
            item,
            kind: AxNodeKind::Item,
            resolved: false,
        });
        self.items.insert(item, key);
        key
    }

    /// Store a resolved node, adding it to the changed list when it differs
    /// from its previous node.
    pub(crate) fn set_node(&mut self, key: AxKey, node: accesskit::Node) {
        let Some(entry) = self.nodes.get_mut(key) else {
            return;
        };

        if !entry.resolved || entry.node != node {
            entry.node = node;
            entry.resolved = true;
            self.changed.push(key);
        }
    }

    /// Make `children` the children of `parent`, in order, deleting its
    /// other children with their subtrees.
    pub(crate) fn set_children(&mut self, parent: AxKey, children: &[AxKey]) {
        for &child in children {
            self.nodes.append(parent, child);
        }

        // The children left before the appended ones are no longer listed.
        let first = children.first().copied();
        while let Some(child) = self.nodes.first_child(parent)
            && Some(child) != first
        {
            self.nodes.delete_node(child);
        }
    }
}
