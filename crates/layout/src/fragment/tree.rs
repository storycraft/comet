use comet_tree::node::NodeKey;
use rustc_hash::FxHashMap;

use super::{Fragment, FragmentKey, TextStyle, builder};
use crate::{slot_tree::SlotTree, tree::LayoutItemKey};

#[derive(Default)]
pub struct FragmentTree {
    pub items: SlotTree<FragmentKey, Fragment>,
    pub text_styles: FxHashMap<NodeKey, TextStyle>,
    /// Shaped text retained for rendering glyph runs by their line/item indices.
    pub inline_layouts: FxHashMap<LayoutItemKey, parley::Layout<Option<NodeKey>>>,
    pub texts: FxHashMap<LayoutItemKey, String>,
    pub(crate) roots: FxHashMap<LayoutItemKey, FragmentKey>,
    pub(crate) snapshots: FxHashMap<LayoutItemKey, builder::Snapshot>,
    pub(crate) layout_cache: FxHashMap<LayoutItemKey, crate::taffy::NodeState>,
}

impl FragmentTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn root_for_item(&self, item: LayoutItemKey) -> Option<FragmentKey> {
        self.roots
            .get(&item)
            .copied()
            .filter(|key| self.items.get(*key).is_some())
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.text_styles.clear();
        self.inline_layouts.clear();
        self.texts.clear();
        self.roots.clear();
        self.snapshots.clear();
        self.layout_cache.clear();
    }

    /// Invalidate a root after changing external font registrations or font data.
    pub fn invalidate(&mut self, root: LayoutItemKey) {
        self.snapshots.remove(&root);
    }
}
