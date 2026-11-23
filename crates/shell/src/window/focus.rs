//! The focused node: moving it, and scrolling it into view.

use comet_layout::style::{bx::Overflow, taffy::Overflow as OverflowKind};
use comet_paint::style::{ScrollLeft, ScrollTop};
use comet_tree::{node::NodeKey, ui::Ui};
use kurbo::{Rect, Vec2};
use reactive_graph::traits::GetUntracked;

use super::{Window, state::update_rw};
use crate::{
    event::FocusEvent,
    pipeline::UiTrees,
    selection::TextPosition,
    style::{Focusable, OnBlur, OnFocus},
};

/// The focused node.
#[derive(Default)]
pub(super) struct FocusState {
    pub focused: Option<NodeKey>,
    /// Focused node to scroll into view, waiting for its first layout.
    pub reveal: Option<NodeKey>,
}

impl Window {
    /// Move the focus to `node`, calling the blur handler of the node losing
    /// it and the focus handler of the node gaining it, and scroll it into
    /// view. Nodes not in the tree clear the focus.
    pub(super) fn set_focus(&mut self, node: Option<NodeKey>) {
        let node = node.filter(|&node| self.attached(node));
        update_rw(self.state.focused_node, node);

        if self.focus.focused == node {
            return;
        }

        let old = core::mem::replace(&mut self.focus.focused, node);
        // The accessibility tree reports the focus.
        self.redraw |= self.pipeline.accessibility_active();
        self.focus.reveal = node;
        self.reveal_focus();

        if let Some(old) = old {
            self.dispatch::<OnBlur>(&[old], None, None, &FocusEvent { related: node });
        }
        if let Some(node) = node {
            self.dispatch::<OnFocus>(&[node], None, None, &FocusEvent { related: old });
        }
    }

    /// Apply a focus change made through the window state, and clear the
    /// focus of a node removed from the tree.
    pub(super) fn sync_focus(&mut self) {
        let wanted = self.state.focused_node.get_untracked();
        if wanted != self.focus.focused {
            self.set_focus(wanted);
        }

        if self.focus.focused.is_some_and(|node| !self.attached(node)) {
            self.set_focus(None);
        }
    }

    /// Scroll the scroll containers holding the node waiting to be revealed,
    /// so that its border box is visible. Kept waiting until the node is
    /// laid out.
    pub(super) fn reveal_focus(&mut self) {
        let Some(node) = self.focus.reveal else {
            return;
        };
        if !self.attached(node) {
            self.focus.reveal = None;
            return;
        }

        let scrolls = {
            let tree = self.ui.tree().borrow();
            reveal_scrolls(&tree.ui, &self.pipeline.trees, node)
        };
        let Some(scrolls) = scrolls else {
            return;
        };
        self.focus.reveal = None;

        let mut tree = self.ui.tree().borrow_mut();
        for (container, offset) in scrolls {
            tree.set_prop(container, ScrollLeft(offset.x));
            tree.set_prop(container, ScrollTop(offset.y));
        }
    }

    /// Focus the next focusable node in tree order, or the previous one,
    /// wrapping around. Without focus, start after the selection, or before
    /// it going back, if any: from its end or its start in tree order,
    /// whichever way it was made.
    pub(super) fn move_focus(&mut self, forward: bool) {
        let ends = match (self.focus.focused, self.selection.current) {
            (None, Some(selection)) => {
                [selection.anchor, selection.extent].map(|position| Some(TreePoint::of(position)))
            }
            _ => [None, None],
        };
        let (order, ends) = self.focus_order(ends);
        let len = order.len();
        if len == 0 {
            return;
        }

        let next = match self
            .focus
            .focused
            .and_then(|focus| order.iter().position(|&node| node == focus))
        {
            Some(index) if forward => (index + 1) % len,
            Some(index) => (index + len - 1) % len,
            None => match ends {
                // The first focusable node after the selection.
                Some((_, end)) if forward => (end.before + usize::from(end.focusable)) % len,
                // The last focusable node before the selection.
                Some((start, _)) => (start.before + len - 1) % len,
                None if forward => 0,
                None => len - 1,
            },
        };
        self.set_focus(Some(order[next]));
    }

    /// The focusable nodes, in tree order, with where the first and the last
    /// in tree order of `ends` are among them, if any is in the tree.
    fn focus_order(
        &self,
        ends: [Option<TreePoint>; 2],
    ) -> (Vec<NodeKey>, Option<(OrderPosition, OrderPosition)>) {
        let tree = self.ui.tree().borrow();
        let ui = &tree.ui;
        let mut order = Vec::new();
        let mut first = None;
        let mut last = None;
        let mut found = |point: TreePoint, before: usize, focusable: bool| {
            if ends.contains(&Some(point)) {
                let found = OrderPosition { before, focusable };
                first.get_or_insert(found);
                last = Some(found);
            }
        };
        // Nodes to visit, and to leave once their subtree is visited.
        let mut pending = vec![TreePoint::At(self.ui.root())];

        while let Some(point) = pending.pop() {
            let TreePoint::At(node) = point else {
                found(point, order.len(), false);
                continue;
            };
            let focusable = ui
                .prop::<Focusable>(node)
                .is_some_and(|focusable| focusable.0);
            found(point, order.len(), focusable);
            if focusable {
                order.push(node);
            }

            pending.push(TreePoint::After(node));
            let children: Vec<_> = ui.children(node).collect();
            pending.extend(children.into_iter().rev().map(TreePoint::At));
        }

        (order, first.zip(last))
    }
}

/// A point in tree order.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TreePoint {
    /// At a node, before its subtree.
    At(NodeKey),
    /// After the subtree of a node.
    After(NodeKey),
}

impl TreePoint {
    /// Where a text position is: at the box of its inline content, or after
    /// the block box its run follows.
    fn of(position: TextPosition) -> Self {
        match position.run {
            Some(run) => Self::After(run),
            None => Self::At(position.node),
        }
    }
}

/// Where a node is among the focusable nodes in tree order.
#[derive(Clone, Copy)]
struct OrderPosition {
    /// Focusable nodes before it.
    before: usize,
    /// Whether it is focusable itself.
    focusable: bool,
}

/// The scroll offsets bringing the border box of `node` into view in the
/// scroll containers holding it, from the last layout: each container,
/// from the innermost out, scrolls as little as possible, showing the start
/// of the box when it does not fit. Only containers that move are returned.
///
/// `None` when the node or one of its ancestors has no laid out fragment.
fn reveal_scrolls(ui: &Ui, trees: &UiTrees, node: NodeKey) -> Option<Vec<(NodeKey, Vec2)>> {
    let items = &trees.fragments.items;
    let mut key = trees.box_fragment(node)?;
    let size = items.get(key)?.layout.size;
    let mut rect = Rect::new(0.0, 0.0, size.width, size.height);
    let mut scrolls = Vec::new();

    while let Some(parent) = items.parent(key) {
        // Into the scrolled contents of the parent.
        rect = trees
            .prepaint
            .fragment(key)?
            .transform
            .transform_rect_bbox(rect);
        let fragment = items.get(parent)?;
        let mut offset = trees.prepaint.scroll_offset(fragment);

        let container = fragment
            .item
            .is_box()
            .then(|| trees.node(fragment.source))
            .flatten();
        if let Some(container) = container
            && let Some(props) = ui.props(container)
        {
            let overflow = props.get_or_default::<Overflow>().0;
            let (x, y) = (
                overflow.x == OverflowKind::Scroll,
                overflow.y == OverflowKind::Scroll,
            );

            if x || y {
                let range = trees.fragments.scroll_range(parent)?;
                // The props may hold more than the range allows since it
                // shrank.
                let current = Vec2::new(
                    props.get_or_default::<ScrollLeft>().0.clamp(0.0, range.x),
                    props.get_or_default::<ScrollTop>().0.clamp(0.0, range.y),
                );
                // The padding box, in local coordinates.
                let layout = &fragment.layout;
                let view = Rect::new(
                    layout.border.left_top.x,
                    layout.border.left_top.y,
                    layout.size.width - layout.border.right_bottom.x,
                    layout.size.height - layout.border.right_bottom.y,
                );
                let target = Vec2::new(
                    if x {
                        reveal_axis(rect.x0, rect.x1, view.x0, view.x1, current.x)
                            .clamp(0.0, range.x)
                    } else {
                        current.x
                    },
                    if y {
                        reveal_axis(rect.y0, rect.y1, view.y0, view.y1, current.y)
                            .clamp(0.0, range.y)
                    } else {
                        current.y
                    },
                );

                if target != current {
                    scrolls.push((container, target));
                }
                offset = target;
            }
        }

        // Into the local coordinates of the parent.
        rect = rect - offset;
        key = parent;
    }

    Some(scrolls)
}

/// The scroll offset of one axis showing `start..end`, in the coordinates of
/// a container's scrolled contents, inside its visible `view_start..view_end`,
/// in its local coordinates, when scrolled by `offset` now. Moves as little
/// as possible, and shows the start when the range does not fit.
fn reveal_axis(start: f64, end: f64, view_start: f64, view_end: f64, offset: f64) -> f64 {
    let (visible_start, visible_end) = (view_start + offset, view_end + offset);

    if start < visible_start || end - start > view_end - view_start {
        start - view_start
    } else if end > visible_end {
        end - view_end
    } else {
        offset
    }
}
