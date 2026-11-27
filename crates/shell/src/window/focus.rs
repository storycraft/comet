//! The focused node: moving it, and scrolling it into view.

use comet_layout::{
    fragment::{BoxFragmentKey, FragmentRef},
    style::bx::Overflow,
};
use comet_paint::style::{ScrollLeft, ScrollTop};
use comet_tree::{node::NodeKey, ui::Ui};
use kurbo::{Rect, Vec2};
use reactive_graph::traits::GetUntracked;
use taffy::Overflow as OverflowKind;

use super::{Window, state::update_rw};
use crate::{
    event::FocusEvent,
    pipeline::UiTrees,
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
            (None, Some(selection)) => [Some(selection.anchor.node), Some(selection.extent.node)],
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
        ends: [Option<NodeKey>; 2],
    ) -> (Vec<NodeKey>, Option<(OrderPosition, OrderPosition)>) {
        let tree = self.ui.tree().borrow();
        let ui = &tree.ui;
        let mut order = Vec::new();
        let mut first = None;
        let mut last = None;
        let mut pending = vec![self.ui.root()];

        while let Some(node) = pending.pop() {
            let focusable = ui
                .prop::<Focusable>(node)
                .is_some_and(|focusable| focusable.0);
            if ends.contains(&Some(node)) {
                let found = OrderPosition {
                    before: order.len(),
                    focusable,
                };
                first.get_or_insert(found);
                last = Some(found);
            }
            if focusable {
                order.push(node);
            }

            let children: Vec<_> = ui.children(node).collect();
            pending.extend(children.into_iter().rev());
        }

        (order, first.zip(last))
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
/// An inline element or text without a box of its own is revealed by the
/// area covering its inline items, across lines and runs.
///
/// `None` when the node or one of its ancestors has no laid out fragment.
fn reveal_scrolls(ui: &Ui, trees: &UiTrees, node: NodeKey) -> Option<Vec<(NodeKey, Vec2)>> {
    let boxes = &trees.fragments.boxes;
    // The root box is in no scroll container.
    if trees
        .box_fragment(node)
        .is_some_and(|key| boxes.parent(key).is_none())
    {
        return Some(Vec::new());
    }
    // The area to reveal, in the scrolled contents of `holder`.
    let (mut holder, mut rect) = revealed_area(trees, node)?;
    let mut scrolls = Vec::new();

    loop {
        let fragment = boxes.get(holder)?;
        let mut offset = trees.scroll(holder);

        // Anonymous run boxes have the node of the box they split, and
        // inline part boxes never scroll.
        let container = trees
            .is_principal(holder)
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
                let range = trees.fragments.scroll_range(holder)?;
                // The props may hold more than the range allows since it
                // shrank.
                let current = Vec2::new(
                    props.get_or_default::<ScrollLeft>().0.clamp(0.0, range.x),
                    props.get_or_default::<ScrollTop>().0.clamp(0.0, range.y),
                );
                // The padding box, in local coordinates.
                let layout = &fragment.layout.geometry;
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

        // Into the local coordinates of the holder, then the scrolled
        // contents of its parent.
        rect = rect - offset;
        let Some(parent) = boxes.parent(holder) else {
            break;
        };
        rect = trees.transform(holder)?.transform_rect_bbox(rect);
        holder = parent;
    }

    Some(scrolls)
}

/// The border box of the fragments of `node`, in the scrolled contents of
/// the box holding them: the parent of its box, or the box owning the inline
/// layout holding its inline items. Items in the runs of a box split by
/// blocks are held by that box.
fn revealed_area(trees: &UiTrees, node: NodeKey) -> Option<(BoxFragmentKey, Rect)> {
    let fragments = &trees.fragments;

    if let Some(key) = trees.box_fragment(node) {
        let size = fragments.get(key)?.layout.geometry.size;
        let rect = Rect::new(0.0, 0.0, size.width, size.height);
        let parent = fragments.boxes.parent(key)?;

        return Some((parent, trees.transform(key)?.transform_rect_bbox(rect)));
    }

    let item = trees.inputs.item_for_node(node)?;
    let pieces: Vec<_> = fragments
        .fragments(&trees.inputs, item)
        .filter_map(|fragment| {
            let FragmentRef::Inline { owner, .. } = fragment else {
                return None;
            };
            let geometry = fragments.geometry(fragment)?;
            let location = geometry.location;

            Some((
                owner,
                geometry.border_box() + Vec2::new(location.x, location.y),
            ))
        })
        .collect();
    let &(first, _) = pieces.first()?;

    if pieces.iter().all(|&(owner, _)| owner == first) {
        let rect = pieces
            .into_iter()
            .map(|(_, rect)| rect)
            .reduce(|a, b| a.union(b))?;
        return Some((first, rect));
    }

    // Across runs: into the scrolled contents of the box they split.
    let holder = fragments.boxes.parent(first)?;
    let mut area: Option<Rect> = None;
    for (owner, rect) in pieces {
        let transform = trees.transform(owner)?;
        let rect = transform.transform_rect_bbox(rect - trees.scroll(owner));
        area = Some(area.map_or(rect, |area| area.union(rect)));
    }

    Some((holder, area?))
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
