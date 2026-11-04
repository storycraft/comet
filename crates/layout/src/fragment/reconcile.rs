use comet_tree::node::NodeKey;
use rustc_hash::FxHashMap;

use super::{FragmentItem, FragmentKey, emit::Desired, snapshot::Snapshot, tree::FragmentTree};
use crate::tree::LayoutItemKey;

type Identity = (LayoutItemKey, Option<NodeKey>, FragmentItem);

/// Flat duplicate chains avoid allocating a vector for every fragment identity.
/// Buffers retain capacity between builds; no fragment data survives a call.
#[derive(Default)]
pub(super) struct Reconciler {
    old: Vec<Option<FragmentKey>>,
    next: Vec<Option<usize>>,
    reusable: FxHashMap<Identity, usize>,
    result: Vec<FragmentKey>,
    last: FxHashMap<NodeKey, FragmentKey>,
}

impl Reconciler {
    pub fn apply(
        &mut self,
        tree: &mut FragmentTree,
        old_root: Option<FragmentKey>,
        desired: &mut Desired,
        snapshot: &Snapshot,
    ) -> FragmentKey {
        if let Some(root) = old_root {
            for key in tree.items.descendants(root) {
                let fragment = &tree.items[key];
                let identity = (fragment.source, fragment.node, fragment.item.clone());
                let index = self.old.len();
                self.next.push(self.reusable.insert(identity, index));
                self.old.push(Some(key));
            }
        }

        // Detach all nodes before deleting parents so reusable descendants survive.
        for &key in self.old.iter().flatten() {
            let source = tree.items[key].source;

            if !snapshot.contains(source) {
                tree.remove_source(source);
            }

            tree.items.remove_parent(key);
        }

        for (parent, fragment) in desired.drain() {
            let identity = (fragment.source, fragment.node, fragment.item.clone());
            let key = if let Some(index) = self.reusable.remove(&identity) {
                if let Some(next) = self.next[index] {
                    self.reusable.insert(identity, next);
                }

                let key = self.old[index].take().unwrap();
                tree.items[key] = fragment;

                key
            } else {
                tree.items.insert(fragment)
            };

            if let Some(parent) = parent {
                tree.items.append(self.result[parent], key);
            }

            if !matches!(tree.items[key].item, FragmentItem::Line { .. })
                && let Some(node) = tree.items[key].node
                && let Some(prev) = self.last.insert(node, key)
            {
                tree.items[prev].link.next = Some(key);
                tree.items[key].link.prev = Some(prev);
            }

            self.result.push(key);
        }

        for key in self.old.drain(..).flatten() {
            tree.items.delete_node(key);
        }

        let root = self.result[0];
        self.next.clear();
        self.reusable.clear();
        self.result.clear();
        self.last.clear();

        root
    }
}
