use std::sync::Arc;

use comet_tree::{node::NodeKey, ui::Ui};

use super::{TextStyle, conv, default_text_style};
use crate::style::geometry::UnitResolveContext;

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
