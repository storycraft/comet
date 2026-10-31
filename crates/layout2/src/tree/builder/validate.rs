use comet_tree::ui::Ui;
use rustc_hash::FxHashSet;

use crate::tree::{InlineIns, LayoutItem, LayoutItemKey, LayoutItemTy, LayoutTree};

// Validate before clearing retained buffers. References never authorize deletion.
pub(super) fn valid(
    ui: &Ui,
    tree: &LayoutTree,
    key: LayoutItemKey,
    boxes: &mut FxHashSet<LayoutItemKey>,
) -> bool {
    let Some(item) = tree.items.get(key) else {
        return false;
    };
    if item.node.is_some_and(|node| ui.node(node).is_none()) {
        return false;
    }

    let LayoutItemTy::Inline(inline) = &item.ty else {
        return true;
    };
    let mut depth = 0_usize;
    let mut offset = 0;
    boxes.clear();

    for instruction in &inline.instructions {
        match instruction {
            InlineIns::PushInline(node) => {
                if ui.node(*node).is_none() {
                    return false;
                }
                depth += 1;
            }
            InlineIns::PopInline => {
                let Some(remaining) = depth.checked_sub(1) else {
                    return false;
                };
                depth = remaining;
            }
            InlineIns::Text(range) => {
                if range.start != offset || inline.text.get(range.clone()).is_none() {
                    return false;
                }
                offset = range.end;
            }
            InlineIns::InlineBlock(child) => {
                if tree.items.parent(*child) != Some(key)
                    || !matches!(
                        tree.items.get(*child),
                        Some(LayoutItem {
                            ty: LayoutItemTy::Block,
                            ..
                        })
                    )
                    || !boxes.insert(*child)
                {
                    return false;
                }
            }
        }
    }

    depth == 0
        && offset == inline.text.len()
        && tree.items.children(key).all(|child| boxes.contains(&child))
}
