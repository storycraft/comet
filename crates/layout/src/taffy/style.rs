use comet_tree::{node::NodeKey, ui::Ui};

use super::conv;
use crate::style::{LayoutConstraint, bx};

/// Box properties never inherit. Missing properties use their declared defaults.
pub fn resolve(ui: &Ui, node: Option<NodeKey>, cx: &LayoutConstraint) -> taffy::Style {
    let mut style = taffy::Style {
        display: taffy::Display::Block,
        ..Default::default()
    };
    let Some(node) = node else { return style };

    if ui
        .prop::<bx::DisplayInner>(node)
        .is_some_and(|v| *v == bx::DisplayInner::FlowRoot)
    {
        style.display = taffy::Display::FlowRoot;
    }

    macro_rules! value {
        ($prop:ident) => {
            ui.prop::<bx::$prop>(node)
                .map(|v| v.0)
                .unwrap_or_else(|| bx::$prop::default().0)
        };
    }

    style.position = value!(Position);
    style.box_sizing = value!(BoxSizing);
    style.overflow = conv::to_point(value!(Overflow), |v| v);
    style.inset = conv::to_rect(value!(Inset), |v| conv::length_auto(v, cx));
    style.size = conv::to_size(value!(Size), |v| conv::dimension(v, cx));
    style.min_size = conv::to_size(value!(MinSize), |v| conv::length_auto(v, cx));
    style.max_size = conv::to_size(value!(MaxSize), |v| conv::length_auto(v, cx));
    style.aspect_ratio = ui.prop::<bx::AspectRatio>(node).map(|v| v.0);
    style.margin = conv::to_rect(value!(Margin), |v| conv::length_auto(v, cx));
    style.padding = conv::to_rect(value!(Padding), |v| conv::length(v, cx));
    style.border = conv::to_rect(value!(Border), |v| conv::length(v, cx));

    style
}
