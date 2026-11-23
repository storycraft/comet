//! Pointer input: hit testing, hover, clicks and wheel scrolling.

use comet_layout::style::{bx::Overflow, taffy::Overflow as OverflowKind};
use comet_paint::style::{ScrollLeft, ScrollTop};
use comet_tree::node::NodeKey;
use kurbo::Point;
use ui_events::pointer::PointerButton;

use super::{Window, state::update};
use crate::{
    event::{ClickEvent, Modifiers, PointerEvent, PointerInfo, ScrollDelta},
    style::{
        Focusable, OnClick, OnPointerDown, OnPointerEnter, OnPointerLeave, OnPointerMove,
        OnPointerUp, OnScroll,
    },
};

/// What the window knows of the pointer.
#[derive(Default)]
pub(super) struct PointerState {
    /// Pointer position in logical units, the units of the layout.
    pub cursor: Option<Point>,
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
                self.pointer.cursor = Some(position);
                self.set_pointer_inside(true);
                self.update_hover();

                let (path, local) = self.target_path(position);
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
                self.pointer.cursor = Some(position);

                let (path, local) = self.target_path(position);
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
                        self.start_selection(position, button.state.count, extend);
                    }
                }
            }
            PointerEvent::Up(button) => {
                let position = button.state.logical_point();
                self.pointer.info = Some(button.pointer);
                self.pointer.cursor = Some(position);

                let (path, local) = self.target_path(position);
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
                    && self.hit_path(position).contains(&pressed)
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
                self.pointer.cursor = None;
                self.set_pointer_inside(false);
                self.update_hover();
                self.window_pointer(event, None);
            }
            PointerEvent::Scroll(scroll) => {
                let position = scroll.state.logical_point();
                self.pointer.info = Some(scroll.pointer);
                self.pointer.cursor = Some(position);

                let (path, local) = self.target_path(position);
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
        let path = self
            .pointer
            .cursor
            .map(|cursor| self.hit_path(cursor))
            .unwrap_or_default();
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

    /// Scroll the innermost scroll container of `path` that can still move
    /// by `delta`, clamped to how far its content overflows in the last
    /// layout. Scrolling down moves the content up. The scroll is painted in
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
                return;
            }
        }
    }

    /// The node capturing the pointer, or the node under `position`, with
    /// its ancestors, innermost first, and the position in the local
    /// coordinates of the fragment hit.
    fn target_path(&mut self, position: Point) -> (Vec<NodeKey>, Option<Point>) {
        if let Some(capture) = self.pointer.capture.filter(|&node| self.attached(node)) {
            return (self.ancestors(Some(capture)), None);
        }

        let hit = self.pipeline.hit(position);
        (
            self.ancestors(hit.map(|hit| hit.node)),
            hit.map(|hit| hit.position),
        )
    }

    /// The node under `position` and its ancestors, innermost first, from
    /// the last layout.
    fn hit_path(&mut self, position: Point) -> Vec<NodeKey> {
        let hit = self.pipeline.hit(position);

        self.ancestors(hit.map(|hit| hit.node))
    }
}
