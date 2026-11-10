//! Retained box fragments: detaching, reusing and deleting them.

use super::{Engine, FragmentItem, FragmentKey};

impl Engine<'_> {
    /// Whether a box fragment is the current box of a live box generating item.
    fn is_live_box(&self, fragment: FragmentKey) -> bool {
        let source = self.fragments.items[fragment].source;

        self.tree.items.get(source).is_some()
            && self.is_box(source)
            && self
                .fragments
                .states
                .get(source)
                .is_some_and(|state| state.fragment == fragment)
    }

    /// Drop the state of the item owning a box fragment, if it is its box.
    fn drop_state(&mut self, fragment: FragmentKey) {
        let source = self.fragments.items[fragment].source;

        if self
            .fragments
            .states
            .get(source)
            .is_some_and(|state| state.fragment == fragment)
        {
            self.fragments.states.remove(source);
            self.fragments.inline_layouts.remove(source);
        }
    }

    /// Delete the descendants of a fragment. Live box fragments are detached
    /// and kept for reuse; state of dead boxes is dropped.
    pub fn release_children(&mut self, fragment: FragmentKey) {
        let mut pending: Vec<FragmentKey> = self.fragments.items.children(fragment).collect();

        while let Some(key) = pending.pop() {
            if self.fragments.items[key].item == FragmentItem::Box {
                if self.is_live_box(key) {
                    self.fragments.items.remove_parent(key);
                    self.scratch.detached.push(key);
                    continue;
                }

                self.drop_state(key);
            } else {
                let source = self.fragments.items[key].source;

                if self.fragments.first.get(source) == Some(&key) {
                    self.fragments.first.remove(source);
                }
            }

            pending.extend(self.fragments.items.children(key));
        }

        while let Some(child) = self.fragments.items.first_child(fragment) {
            self.fragments.items.delete_node(child);
        }
    }

    /// Delete a fragment, keeping its live box descendants.
    pub fn release(&mut self, fragment: FragmentKey) {
        if self.fragments.items.get(fragment).is_none() {
            return;
        }

        self.release_children(fragment);
        self.fragments.items.delete_node(fragment);
    }

    /// Delete box fragments that were detached and never attached again.
    pub fn sweep(&mut self) {
        let root = self.fragments.box_fragment(self.root);

        while let Some(fragment) = self.scratch.detached.pop() {
            if self.fragments.items.get(fragment).is_none()
                || self.fragments.items.parent(fragment).is_some()
                || Some(fragment) == root
            {
                continue;
            }

            self.drop_state(fragment);
            self.release(fragment);
        }
    }
}
