use accesskit::NodeId;
use comet_layout::{
    fragment::FragmentKey,
    input::{LayoutInputKey, tree::LayoutInputTree},
};
use comet_tree::node::NodeKey;
use slotmap::{Key, KeyData, SecondaryMap};

use crate::A11yFlags;

/// The accessibility node of one fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct A11yNode {
    /// The node last resolved for the fragment.
    pub node: accesskit::Node,
}

/// Accessibility nodes of a layout, following the layout input tree in the
/// order of the UI: each item has a node for each of its fragments, its box
/// or its inline and glyph run fragments on every line, holding the nodes of
/// its child items. Line boxes and anonymous blocks have no nodes.
///
/// Node ids are the fragment keys, so nodes keep their ids as long as their
/// fragments keep their keys. Built and updated by an
/// [`A11yTreeBuilder`](crate::builder::A11yTreeBuilder).
///
/// Keys are generational: nodes left behind by deleted fragments are never
/// returned for fragments reusing their slots, and are overwritten when those
/// are resolved. A build from a root fragment without a node drops them.
#[derive(Debug, Default)]
pub struct A11yTree {
    pub(crate) nodes: SecondaryMap<FragmentKey, A11yNode>,
    /// Accessibility changes of the layout input items, cleared by the
    /// builder once visited.
    pub(crate) flags: SecondaryMap<LayoutInputKey, A11yFlags>,
    /// Fragments whose nodes changed since the last update.
    pub(crate) changed: Vec<FragmentKey>,
}

impl A11yTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fragments whose nodes changed since the last
    /// [update](Self::take_update), possibly more than once each.
    pub fn changed(&self) -> &[FragmentKey] {
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

    /// The node of a fragment.
    pub fn get(&self, key: FragmentKey) -> Option<&A11yNode> {
        self.nodes.get(key)
    }

    /// Accessibility changes of a layout input item.
    pub fn flags(&self, key: LayoutInputKey) -> A11yFlags {
        self.flags.get(key).copied().unwrap_or_default()
    }

    /// Mark a layout input item [`A11yFlags::DIRTY`], and its ancestors
    /// [`A11yFlags::CHILD_DIRTY`] up to the root item, so the next build
    /// resolves its nodes again.
    pub fn mark(&mut self, inputs: &LayoutInputTree, key: LayoutInputKey) {
        self.insert_flags(key, A11yFlags::DIRTY);

        let mut ancestor = inputs.items.parent(key);
        while let Some(key) = ancestor {
            // Ancestors of a marked item are marked already.
            if self.flags(key).contains(A11yFlags::CHILD_DIRTY) {
                break;
            }
            self.insert_flags(key, A11yFlags::CHILD_DIRTY);
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

    fn insert_flags(&mut self, key: LayoutInputKey, flags: A11yFlags) {
        match self.flags.get_mut(key) {
            Some(current) => current.insert(flags),
            None => {
                self.flags.insert(key, flags);
            }
        }
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.flags.clear();
        self.changed.clear();
    }
}

/// The node id of a fragment.
pub fn node_id(key: FragmentKey) -> NodeId {
    NodeId(key.data().as_ffi())
}

/// The fragment of a node id, such as the target of an action request.
pub fn fragment_key(id: NodeId) -> FragmentKey {
    KeyData::from_ffi(id.0).into()
}
