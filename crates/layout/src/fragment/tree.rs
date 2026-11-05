use comet_tree::node::NodeKey;
use rustc_hash::FxHashMap;

use super::{Fragment, FragmentKey, TextStyle, snapshot::Snapshot};
use crate::{slot_tree::SlotTree, tree::LayoutItemKey};

#[derive(Default)]
pub struct FragmentTree {
    pub items: SlotTree<FragmentKey, Fragment>,
    pub text_styles: FxHashMap<NodeKey, TextStyle>,
    /// Shaped text retained for rendering glyph runs by their line/item indices.
    pub inline_layouts: FxHashMap<LayoutItemKey, parley::Layout<Option<NodeKey>>>,
    pub texts: FxHashMap<LayoutItemKey, String>,
    pub(crate) snapshots: FxHashMap<LayoutItemKey, Snapshot>,
    pub(crate) layout_cache: FxHashMap<LayoutItemKey, crate::taffy::NodeState>,
}

impl FragmentTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// The constraints used to build the root containing this fragment.
    pub fn layout_constraint(
        &self,
        mut key: FragmentKey,
    ) -> Option<&crate::style::LayoutConstraint> {
        while let Some(parent) = self.items.parent(key) {
            key = parent;
        }

        let source = self.items.get(key)?.source;

        self.snapshots
            .get(&source)
            .map(|snapshot| &snapshot.constraint)
    }

    pub(crate) fn remove_source(&mut self, key: LayoutItemKey) {
        self.inline_layouts.remove(&key);
        self.texts.remove(&key);
        self.layout_cache.remove(&key);
    }

    pub(crate) fn remove_root(&mut self, key: FragmentKey) {
        let root = self.items[key].source;
        if let Some(snapshot) = self.snapshots.remove(&root) {
            for key in snapshot.keys() {
                self.remove_source(key);
            }
        }

        self.items.delete_node(key);
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.text_styles.clear();
        self.inline_layouts.clear();
        self.texts.clear();
        self.snapshots.clear();
        self.layout_cache.clear();
    }

    /// Invalidate a root after changing external font registrations or font data.
    pub fn invalidate(&mut self, root: LayoutItemKey) {
        self.snapshots.remove(&root);
    }
}
