use comet_tree::{node::NodeKey, ui::Ui};
use rustc_hash::FxHashMap;

use super::{TextStyle, conv, default_text_style};
use crate::{
    style::LayoutConstraint,
    tree::{LayoutItemKey, LayoutTree},
};

/// Resolve from the UI ancestors down, caching computed inherited values.
pub fn resolve(
    ui: &Ui,
    node: NodeKey,
    cx: &LayoutConstraint,
    cache: &mut FxHashMap<NodeKey, TextStyle>,
) -> TextStyle {
    let mut pending = Vec::new();
    let mut ancestor = Some(node);
    let mut style = default_text_style();

    while let Some(node) = ancestor {
        if let Some(cached) = cache.get(&node) {
            style = cached.clone();
            break;
        }

        pending.push(node);
        ancestor = ui.parent(node);
    }

    for node in pending.into_iter().rev() {
        style = conv::text_style(ui, node, cx, &style);
        cache.insert(node, style.clone());
    }

    style
}

/// Anonymous layout items inherit the nearest source element's text style.
pub(crate) fn resolve_for_item(
    ui: &Ui,
    tree: &LayoutTree,
    key: LayoutItemKey,
    cx: &LayoutConstraint,
    cache: &mut FxHashMap<NodeKey, TextStyle>,
) -> Option<TextStyle> {
    let mut ancestor = Some(key);

    while let Some(key) = ancestor {
        if let Some(node) = tree.items[key].ty.node() {
            return Some(resolve(ui, node, cx, cache));
        }

        ancestor = tree.items.parent(key);
    }

    None
}
