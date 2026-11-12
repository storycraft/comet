//! Retained fragments: detaching, reusing and deleting them.
//!
//! Box fragments of live items and anonymous blocks of live owners are
//! retained with their state. Every other fragment is transient and replaced
//! when its parent is laid out again.

use super::{Engine, FragmentItem, FragmentKey};

impl Engine<'_> {
    /// Whether a fragment is retained: the current box of a live box generating
    /// item, or an anonymous block of a live owner.
    fn is_retained(&self, fragment: FragmentKey) -> bool {
        if self.fragments.get_state(fragment).is_none() {
            return false;
        }

        let item = &self.fragments.items[fragment];

        match item.item {
            FragmentItem::Box { .. } => {
                self.tree.items.get(item.source).is_some()
                    && self.is_box(item.source)
                    && self.fragments.first.get(item.source) == Some(&fragment)
            }
            FragmentItem::AnonymousBlock { .. } => self.tree.items.get(item.source).is_some(),
            _ => false,
        }
    }

    /// Drop the state, inline layout and lookups of a fragment.
    pub fn drop_state(&mut self, fragment: FragmentKey) {
        let item = &self.fragments.items[fragment];
        let source = item.source;

        if let Some(state) = item.item.state() {
            self.fragments.states.remove(state);
        }

        if self.fragments.first.get(source) == Some(&fragment) {
            self.fragments.first.remove(source);
        }
    }

    /// Delete the descendants of a fragment. Retained fragments are detached
    /// and kept for reuse; the state of other fragments is dropped.
    pub fn release_children(&mut self, fragment: FragmentKey) {
        self.release_after(fragment, None);
    }

    /// Delete the children of a fragment following `after`, or all of them
    /// when `after` is `None`, like [`Self::release_children`]. Children up to
    /// `after` are kept in place.
    pub fn release_after(&mut self, fragment: FragmentKey, after: Option<FragmentKey>) {
        let tail = |items: &comet_slotmap_tree::SlotTree<FragmentKey, _>| match after {
            Some(key) => items.next_sibling(key),
            None => items.first_child(fragment),
        };
        let mut pending: Vec<FragmentKey> = self
            .fragments
            .items
            .cursor(tail(&self.fragments.items))
            .collect();

        while let Some(key) = pending.pop() {
            if self.is_retained(key) {
                self.fragments.items.remove_parent(key);
                self.scratch.detached.push(key);
                continue;
            }

            self.drop_state(key);
            pending.extend(self.fragments.items.children(key));
        }

        // Retained fragments left the sibling chain, so the rest is deleted.
        while let Some(child) = tail(&self.fragments.items) {
            self.fragments.items.delete_node(child);
        }
    }

    /// Delete a fragment, keeping its retained descendants.
    pub fn release(&mut self, fragment: FragmentKey) {
        if self.fragments.items.get(fragment).is_none() {
            return;
        }

        self.release_children(fragment);
        self.fragments.items.delete_node(fragment);
    }

    /// Check that retained state matches the fragment tree: every state
    /// belongs to the live fragment it points to, and every retained fragment
    /// below the root has state.
    #[cfg(debug_assertions)]
    pub fn check(&self) {
        for (key, state) in &self.fragments.states {
            assert_eq!(
                self.fragments
                    .items
                    .get(state.fragment)
                    .and_then(|fragment| fragment.item.state()),
                Some(key),
                "state of a deleted fragment"
            );
        }

        if let Some(root) = self.fragments.box_fragment(self.root) {
            for key in self.fragments.items.descendants(root) {
                if self.fragments.items[key].item.state().is_some() {
                    assert!(
                        self.fragments.get_state(key).is_some(),
                        "retained fragment without state"
                    );
                }
            }
        }
    }

    /// Delete retained fragments that were detached and never attached again.
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
