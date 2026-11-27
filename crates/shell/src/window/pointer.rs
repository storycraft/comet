//! Pointer input: hit testing, hover, the cursor, clicks and wheel
//! scrolling.

use comet_layout::style::bx::Overflow;
use comet_paint::style::{ScrollLeft, ScrollTop};
use comet_tree::node::{Node, NodeKey};
use kurbo::Point;
use taffy::Overflow as OverflowKind;
use ui_events::{
    ScrollDelta,
    keyboard::Modifiers,
    pointer::{PointerButton, PointerEvent, PointerInfo},
};

use super::{Window, state::update};
use crate::{
    cursor::CursorIcon,
    event::ClickEvent,
    pipeline::Hit,
    style::{
        Cursor, Focusable, OnClick, OnPointerDown, OnPointerEnter, OnPointerLeave, OnPointerMove,
        OnPointerUp, OnScroll,
    },
};

/// What the window knows of the pointer.
#[derive(Default)]
pub(super) struct PointerState {
    /// Pointer position in logical units, the units of the layout.
    pub cursor: Option<Point>,
    /// The path to the fragment under the pointer at its last event, from
    /// the root down, hit tested once per event. Empty outside the window or
    /// over no fragment.
    pub path: Vec<Hit>,
    /// The pointer of the last pointer event.
    pub info: Option<PointerInfo>,
    /// Nodes under the pointer, innermost first.
    pub hovered: Vec<NodeKey>,
    /// Node a pointer button was pressed on, clicked when released over it.
    pub pressed: Option<NodeKey>,
    /// Node capturing the pointer.
    pub capture: Option<NodeKey>,
}

impl Window {
    /// See [`Window::pointer_event`].
    pub(super) fn handle_pointer(&mut self, event: &PointerEvent) {
        match event {
            PointerEvent::Move(update) => {
                let position = update.current.logical_point();
                self.pointer.info = Some(update.pointer);
                self.hit_test(Some(position));
                self.set_pointer_inside(true);
                self.update_hover();

                let (path, local) = self.target_path();
                let prevented = self.dispatch_pointer::<OnPointerMove>(
                    &path,
                    Some(position),
                    local,
                    event,
                    Some(update),
                );

                if self.selection.selecting && !prevented {
                    self.extend_selection(position);
                }
            }
            PointerEvent::Down(button) => {
                let position = button.state.logical_point();
                self.pointer.info = Some(button.pointer);
                self.hit_test(Some(position));

                let (path, local) = self.target_path();
                let prevented = self.dispatch_pointer::<OnPointerDown>(
                    &path,
                    Some(position),
                    local,
                    event,
                    Some(button),
                );
                self.pointer.pressed = path.first().copied();

                if !prevented {
                    // Focus the nearest focusable node, or clear the focus.
                    let focus = {
                        let tree = self.ui.tree().borrow();
                        path.iter().copied().find(|&node| {
                            tree.ui
                                .prop::<Focusable>(node)
                                .is_some_and(|focusable| focusable.0)
                        })
                    };
                    self.set_focus(focus);

                    if button.button == Some(PointerButton::Primary) {
                        let extend = button.state.modifiers.contains(Modifiers::SHIFT);
                        self.start_selection(button.state.count, extend);
                    }
                }
            }
            PointerEvent::Up(button) => {
                let position = button.state.logical_point();
                self.pointer.info = Some(button.pointer);
                self.hit_test(Some(position));

                let (path, local) = self.target_path();
                self.dispatch_pointer::<OnPointerUp>(
                    &path,
                    Some(position),
                    local,
                    event,
                    Some(button),
                );
                self.pointer.capture = None;
                self.selection.selecting = false;

                // Clicked when released over the node it was pressed on.
                if let Some(pressed) = self.pointer.pressed.take()
                    && self.hit_ancestors().contains(&pressed)
                {
                    let path = self.ancestors(Some(pressed));
                    let click = ClickEvent {
                        button: button.button,
                        count: button.state.count.max(1),
                    };
                    self.dispatch::<OnClick>(&path, Some(position), None, &click);
                }
            }
            PointerEvent::Cancel(pointer) => {
                self.pointer.info = Some(*pointer);
                self.pointer.pressed = None;
                self.pointer.capture = None;
                self.selection.selecting = false;
                self.window_pointer(event, None);
            }
            PointerEvent::Enter(pointer) => {
                self.pointer.info = Some(*pointer);
                self.set_pointer_inside(true);
                self.window_pointer(event, None);
            }
            PointerEvent::Leave(pointer) => {
                self.pointer.info = Some(*pointer);
                self.hit_test(None);
                self.set_pointer_inside(false);
                self.update_hover();
                self.window_pointer(event, None);
            }
            PointerEvent::Scroll(scroll) => {
                let position = scroll.state.logical_point();
                self.pointer.info = Some(scroll.pointer);
                self.hit_test(Some(position));

                let (path, local) = self.target_path();
                if !self.dispatch_pointer::<OnScroll>(
                    &path,
                    Some(position),
                    local,
                    event,
                    Some(scroll),
                ) {
                    self.scroll(&path, scroll.delta);
                }
            }
            PointerEvent::Gesture(gesture) => {
                self.pointer.info = Some(gesture.pointer);
                self.window_pointer(event, Some(gesture.state.logical_point()));
            }
        }
    }

    /// Move the pointer to `position`, hit testing the path under it from
    /// the last layout, or leave the window with `None`.
    fn hit_test(&mut self, position: Option<Point>) {
        self.pointer.cursor = position;

        match position {
            Some(position) => self.pipeline.hit_path(position, &mut self.pointer.path),
            None => self.pointer.path.clear(),
        }
    }

    /// The node under the pointer and its ancestors, innermost first.
    fn hit_ancestors(&self) -> Vec<NodeKey> {
        self.ancestors(self.pointer.path.last().map(|hit| hit.node))
    }

    fn set_pointer_inside(&mut self, inside: bool) {
        update(
            self.state.pointer_inside,
            self.writers.pointer_inside,
            inside,
        );
    }

    /// Call the leave handlers of the nodes the pointer left, innermost
    /// first, then the enter handlers of those it entered, outermost first.
    fn update_hover(&mut self) {
        let path = self.hit_ancestors();
        let hovered = &self.pointer.hovered;
        let left: Vec<_> = hovered
            .iter()
            .copied()
            .filter(|node| !path.contains(node))
            .collect();
        let entered: Vec<_> = path
            .iter()
            .rev()
            .copied()
            .filter(|node| !hovered.contains(node))
            .collect();
        self.pointer.hovered = path;
        self.update_cursor();

        let (Some(info), cursor) = (self.pointer.info, self.pointer.cursor) else {
            return;
        };
        for node in left {
            self.dispatch::<OnPointerLeave>(&[node], cursor, None, &info);
        }
        for node in entered {
            self.dispatch::<OnPointerEnter>(&[node], cursor, None, &info);
        }
    }

    /// Follow the [`Cursor`] under the pointer with
    /// [`WindowState::cursor`](super::WindowState::cursor): the first cursor
    /// other than `Auto` from the node hit up the tree, or
    /// [`CursorIcon::Text`] over text and [`CursorIcon::Default`] elsewhere
    /// when no node decides. `None` hides it.
    pub(super) fn update_cursor(&mut self) {
        let leaf = self.pointer.path.last().map(|hit| hit.node);
        let over_text =
            leaf.is_some_and(|node| self.ui.tree().borrow().ui.kind(node) == Some(Node::Text));
        let cursor = self
            .inherited::<Cursor, _>(leaf, |prop| prop.0)
            .unwrap_or(Some(if over_text {
                CursorIcon::Text
            } else {
                CursorIcon::Default
            }));

        update(self.state.cursor, self.writers.cursor, cursor);
    }

    /// Scroll the innermost scroll container of `path` by `delta`, clamped
    /// to how far its content overflows in the last layout. Scrolling down
    /// moves the content up. The scroll never passes to an outer container,
    /// even when the innermost one is at its end. The scroll is painted in
    /// the next frame, which the change requests.
    fn scroll(&mut self, path: &[NodeKey], delta: ScrollDelta) {
        let delta = self.viewport.scroll_delta(delta);
        let mut tree = self.ui.tree().borrow_mut();

        for &node in path {
            let Some(range) = self.pipeline.trees.scroll_range(node) else {
                continue;
            };
            let Some(props) = tree.ui.props(node) else {
                continue;
            };
            let overflow = props.get_or_default::<Overflow>().0;
            if overflow.x != OverflowKind::Scroll && overflow.y != OverflowKind::Scroll {
                continue;
            }
            // The props may hold more than the range allows since it shrank.
            let left = props.get_or_default::<ScrollLeft>().0.min(range.x);
            let top = props.get_or_default::<ScrollTop>().0.min(range.y);

            let new_left = if overflow.x == OverflowKind::Scroll {
                (left - delta.x).clamp(0.0, range.x)
            } else {
                left
            };
            let new_top = if overflow.y == OverflowKind::Scroll {
                (top - delta.y).clamp(0.0, range.y)
            } else {
                top
            };

            if new_left != left || new_top != top {
                tree.set_prop(node, ScrollLeft(new_left));
                tree.set_prop(node, ScrollTop(new_top));
            }
            return;
        }
    }

    /// The node capturing the pointer, or the node under it, with its
    /// ancestors, innermost first, and the pointer in the local coordinates
    /// of the fragment hit.
    fn target_path(&self) -> (Vec<NodeKey>, Option<Point>) {
        if let Some(capture) = self.pointer.capture.filter(|&node| self.attached(node)) {
            return (self.ancestors(Some(capture)), None);
        }

        (
            self.hit_ancestors(),
            self.pointer.path.last().map(|hit| hit.position),
        )
    }
}
