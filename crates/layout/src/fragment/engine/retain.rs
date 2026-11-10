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
        if !self.fragments.states.contains_key(fragment) {
            return false;
        }

        let item = &self.fragments.items[fragment];

        match item.item {
            FragmentItem::Box => {
                self.tree.items.get(item.source).is_some()
                    && self.is_box(item.source)
                    && self.fragments.first.get(item.source) == Some(&fragment)
            }
            FragmentItem::AnonymousBlock => self.tree.items.get(item.source).is_some(),
            _ => false,
        }
    }

    /// Drop the state, inline layout and lookups of a fragment.
    pub fn drop_state(&mut self, fragment: FragmentKey) {
        self.fragments.states.remove(fragment);
        self.fragments.inline_layouts.remove(fragment);
        self.fragments.constraints.remove(fragment);

        let source = self.fragments.items[fragment].source;

        if self.fragments.first.get(source) == Some(&fragment) {
            self.fragments.first.remove(source);
        }
    }

    /// Delete the descendants of a fragment. Retained fragments are detached
    /// and kept for reuse; the state of other fragments is dropped.
    pub fn release_children(&mut self, fragment: FragmentKey) {
        let mut pending: Vec<FragmentKey> = self.fragments.items.children(fragment).collect();

        while let Some(key) = pending.pop() {
            if self.is_retained(key) {
                self.fragments.items.remove_parent(key);
                self.scratch.detached.push(key);
                continue;
            }

            self.drop_state(key);
            pending.extend(self.fragments.items.children(key));
        }

        while let Some(child) = self.fragments.items.first_child(fragment) {
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

    /// Check that retained state matches the fragment tree: every state and
    /// inline layout belongs to a live retained fragment, and every retained
    /// fragment below the root has state.
    #[cfg(debug_assertions)]
    pub fn check(&self) {
        for (fragment, _) in &self.fragments.states {
            assert!(
                self.fragments.items.get(fragment).is_some(),
                "state of a deleted fragment"
            );
        }

        for (fragment, _) in &self.fragments.inline_layouts {
            assert!(
                self.fragments.states.contains_key(fragment),
                "inline layout without state"
            );
        }

        if let Some(root) = self.fragments.box_fragment(self.root) {
            for key in self.fragments.items.descendants(root) {
                if matches!(
                    self.fragments.items[key].item,
                    FragmentItem::Box | FragmentItem::AnonymousBlock
                ) {
                    assert!(
                        self.fragments.states.contains_key(key),
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
