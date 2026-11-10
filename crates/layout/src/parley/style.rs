use std::sync::Arc;

use comet_tree::{node::NodeKey, ui::Ui};
use rustc_hash::FxHashMap;

use super::{TextStyle, conv, default_text_style};
use crate::{
    style::{LayoutConstraint, geometry::UnitResolveContext},
    tree::{LayoutItemKey, LayoutTree},
};

/// Resolve from the UI ancestors down, caching computed inherited values.
/// Equal inherited styles share an allocation; brushes are assigned during shaping.
pub fn resolve(
    ui: &Ui,
    node: NodeKey,
    cx: &LayoutConstraint,
    cache: &mut FxHashMap<NodeKey, Arc<TextStyle>>,
) -> Arc<TextStyle> {
    let mut pending = Vec::new();
    let mut ancestor = Some(node);
    let mut style = None;

    while let Some(node) = ancestor {
        if let Some(cached) = cache.get(&node) {
            style = Some(cached.clone());
            break;
        }

        pending.push(node);
        ancestor = ui.parent(node);
    }

    let mut style = style.unwrap_or_else(|| Arc::new(default_text_style()));
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
    cache: &mut FxHashMap<NodeKey, Arc<TextStyle>>,
) -> Option<Arc<TextStyle>> {
    let mut ancestor = Some(key);

    while let Some(key) = ancestor {
        if let Some(node) = tree.items[key].ty.node() {
            return Some(resolve(ui, node, cx, cache));
        }

        ancestor = tree.items.parent(key);
    }

    None
}

/// Default style for a root without a source parent. Its font size is the root font size.
pub fn root_text_style(root_font_size: f64) -> TextStyle {
    TextStyle {
        font_size: root_font_size as f32,
        ..default_text_style()
    }
}

/// Resolve `node`'s style from the UI root down, without caching.
/// `cx` supplies the root font size and viewport.
pub fn resolve_inherited(ui: &Ui, node: NodeKey, cx: &UnitResolveContext) -> Arc<TextStyle> {
    let mut pending = Vec::new();
    let mut ancestor = Some(node);

    while let Some(node) = ancestor {
        pending.push(node);
        ancestor = ui.parent(node);
    }

    let mut style = Arc::new(root_text_style(cx.root_font_size));
    for node in pending.into_iter().rev() {
        style = conv::resolve_text_style(ui, node, cx, &style);
    }

    style
}
