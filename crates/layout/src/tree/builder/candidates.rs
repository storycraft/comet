use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    ChildLayoutMode, Frame, LayoutItem, LayoutItemKey, LayoutItemTy, LayoutTree, NodeKey, item,
    matches_item,
};

// Queues share one arena and retain their capacity across builds.
#[derive(Default)]
pub struct Candidates {
    queues: FxHashMap<(usize, LayoutItemTy), CandidateQueue>,
    entries: Vec<Candidate>,
    next_index: usize,

    // Lazy indexes can retain moved items; never reuse an item twice per update.
    claimed: FxHashSet<LayoutItemKey>,
}

impl Candidates {
    pub fn find(
        &mut self,
        tree: &LayoutTree,
        frame: &mut Frame,
        desired: LayoutItem,
        text: Option<NodeKey>,
    ) -> Option<LayoutItemKey> {
        if let Some(key) = frame.next
            && matches_item(tree, key, desired, text)
        {
            return Some(key);
        }

        let parent = frame.key;
        let next = frame.next;
        let index = if let Some(index) = frame.index {
            index
        } else {
            let index = self.next_index;
            self.next_index += 1;
            frame.index = Some(index);

            for key in tree.items.cursor(next) {
                self.push(index, tree.items[key].ty, key, false);
            }

            index
        };

        while let Some(key) = self.pop(index, desired.ty) {
            if !self.claimed.contains(&key)
                && tree.items.parent(key) == Some(parent)
                && matches_item(tree, key, desired, text)
            {
                return Some(key);
            }
        }

        None
    }

    pub fn adopt(
        &mut self,
        tree: &LayoutTree,
        frame: &mut Frame,
        desired: LayoutItem,
        text: Option<NodeKey>,
    ) -> Option<LayoutItemKey> {
        let anonymous = item(LayoutItemTy::AnonymousBlock, ChildLayoutMode::Inline);
        let frame_key = frame.key;
        let next = frame.next;
        let index = if let Some(index) = frame.adoption {
            index
        } else {
            let index = self.next_index;
            self.next_index += 1;
            frame.adoption = Some(index);

            for key in tree.items.cursor(next) {
                if tree.items[key] == anonymous {
                    for child in tree.items.children(key) {
                        self.push(index, tree.items[child].ty, child, false);
                    }
                }

                self.push(index, tree.items[key].ty, key, false);
            }

            index
        };

        while let Some(key) = self.pop(index, desired.ty) {
            let parent = tree.items.parent(key);
            let eligible = parent == Some(frame_key)
                || parent.is_some_and(|parent| {
                    !self.claimed.contains(&parent)
                        && tree.items.parent(parent) == Some(frame_key)
                        && tree.items.get(parent) == Some(&anonymous)
                });

            if eligible && !self.claimed.contains(&key) && matches_item(tree, key, desired, text) {
                return Some(key);
            }
        }

        None
    }

    pub fn push(&mut self, index: usize, ty: LayoutItemTy, key: LayoutItemKey, front: bool) {
        let queue = self.queues.entry((index, ty)).or_default();
        let position = self.entries.len();
        self.entries.push(Candidate {
            key,
            next: if front { queue.head } else { None },
        });

        if front {
            queue.head = Some(position);

            if queue.tail.is_none() {
                queue.tail = Some(position);
            }
        } else {
            if let Some(tail) = queue.tail {
                self.entries[tail].next = Some(position);
            } else {
                queue.head = Some(position);
            }

            queue.tail = Some(position);
        }
    }

    fn pop(&mut self, index: usize, ty: LayoutItemTy) -> Option<LayoutItemKey> {
        let queue = self.queues.get_mut(&(index, ty))?;
        let candidate = &self.entries[queue.head?];
        queue.head = candidate.next;

        if queue.head.is_none() {
            queue.tail = None;
        }

        Some(candidate.key)
    }

    pub fn claim(&mut self, key: LayoutItemKey) {
        self.claimed.insert(key);
    }

    pub fn clear(&mut self) {
        self.queues.clear();
        self.entries.clear();
        self.next_index = 0;
        self.claimed.clear();
    }
}

// Inline sources can produce multiple fragments, so preserve candidate order.
#[derive(Default)]
struct CandidateQueue {
    head: Option<usize>,
    tail: Option<usize>,
}

struct Candidate {
    key: LayoutItemKey,
    next: Option<usize>,
}
