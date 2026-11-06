use std::sync::Arc;

use comet_tree::{node::Text, ui::Ui};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::SecondaryMap;

use super::{TextStyle, tree::FragmentTree};
use crate::{
    style::LayoutConstraint,
    taffy::{NodeState, style},
    tree::{LayoutItem, LayoutItemKey, LayoutTree},
};

#[derive(PartialEq)]
pub(crate) struct Snapshot {
    pub(super) constraint: LayoutConstraint,
    inputs: SecondaryMap<LayoutItemKey, Input>,
}

/// Linked topology captures ordering without allocating a child vector per item.
#[derive(PartialEq)]
struct Input {
    item: LayoutItem,
    style: crate::taffy::style::Style,
    text_style: Option<Arc<TextStyle>>,
    text: Option<String>,
    parent: Option<LayoutItemKey>,
    first_child: Option<LayoutItemKey>,
    next_sibling: Option<LayoutItemKey>,
}

impl Snapshot {
    pub fn capture(
        constraint: &LayoutConstraint,
        ui: &Ui,
        layout: &LayoutTree,
        root: LayoutItemKey,
        fragments: &mut FragmentTree,
    ) -> Self {
        let mut inputs = SecondaryMap::new();
        let mut resolved = FxHashMap::default();

        for key in layout.items.descendants(root) {
            let item = layout.items[key];
            let text_style =
                crate::parley::style::resolve_for_item(ui, layout, key, constraint, &mut resolved);
            let mut cx = constraint.clone();

            if let Some(style) = &text_style {
                cx.font_size = style.font_size as f64;
            }

            inputs.insert(
                key,
                Input {
                    item,
                    style: style::resolve(ui, item.ty.node(), &cx),
                    text_style,
                    text: item
                        .ty
                        .node()
                        .and_then(|node| ui.prop::<Text>(node))
                        .map(|text| text.0.clone()),
                    parent: (key != root).then(|| layout.items.parent(key)).flatten(),
                    first_child: layout.items.first_child(key),
                    next_sibling: (key != root)
                        .then(|| layout.items.next_sibling(key))
                        .flatten(),
                },
            );
        }

        fragments.text_styles.extend(resolved);

        Self {
            constraint: constraint.clone(),
            inputs,
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = LayoutItemKey> + '_ {
        self.inputs.keys()
    }

    pub fn contains(&self, key: LayoutItemKey) -> bool {
        self.inputs.contains_key(key)
    }

    pub fn prepare(
        &self,
        layout: &LayoutTree,
        root: LayoutItemKey,
        fragments: &mut FragmentTree,
    ) -> SecondaryMap<LayoutItemKey, NodeState> {
        let previous = fragments.snapshots.remove(root);
        let constraint_changed = previous
            .as_ref()
            .is_none_or(|old| old.constraint != self.constraint);
        let mut dirty = FxHashSet::default();

        for (key, input) in &self.inputs {
            if constraint_changed
                || previous.as_ref().and_then(|old| old.inputs.get(key)) != Some(input)
            {
                let mut ancestor = Some(key);

                while let Some(key) = ancestor {
                    if !dirty.insert(key) || key == root {
                        break;
                    }

                    ancestor = layout.items.parent(key);
                }
            }
        }

        if let Some(previous) = previous {
            for key in previous.keys().filter(|&key| !self.contains(key)) {
                fragments.remove_source(key);
            }
        }

        for (key, input) in &self.inputs {
            let state = fragments
                .layout_cache
                .entry(key)
                .expect("snapshot keys belong to the current layout tree")
                .or_insert_with(|| NodeState::new(input.style.clone()));
            state.style.clone_from(&input.style);

            if dirty.contains(&key) {
                state.cache.clear();
                fragments.inline_layouts.remove(key);
            }
        }

        std::mem::take(&mut fragments.layout_cache)
    }
}
