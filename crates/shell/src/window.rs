//! A platform independent window showing a reactive UI.

use core::iter::successors;
use std::time::Instant;

use accesskit::{Action, ActionRequest, TreeUpdate};
use comet_layout::style::{bx::Overflow, taffy::Overflow as OverflowKind};
use comet_paint::style::{ScrollLeft, ScrollTop};
use comet_reactivity::UiCx;
use comet_tree::{node::NodeKey, ui::Ui};
use dpi::PhysicalSize;
use kurbo::{Point, Rect, Size, Vec2};
use reactive_graph::{
    owner::{Owner, provide_context, use_context},
    signal::{ReadSignal, RwSignal, Trigger, WriteSignal, signal},
    traits::{GetUntracked, Notify, Set},
};

use crate::{
    event::{
        ClickEvent, EventCx, FocusEvent, Key, KeyState, KeyboardEvent, Modifiers, NamedKey,
        PointerEvent, PointerInfo, Request, ScrollDelta,
    },
    listeners::{Listeners, Phase, WindowListener},
    pipeline::{Pipeline, UiTrees},
    runtime::UiRoot,
    style::{
        Focusable, HandlerProp, OnBlur, OnClick, OnFocus, OnKeyDown, OnKeyUp, OnPointerDown,
        OnPointerEnter, OnPointerLeave, OnPointerMove, OnPointerUp, OnScroll,
    },
};

/// Distance scrolled by one line of a mouse wheel, in logical units.
const WHEEL_LINE: f64 = 40.0;

/// Window state as signals, for components to follow. Provided as context
/// to the window's components: see [`use_window`].
#[derive(Clone, Copy)]
pub struct WindowState {
    /// Size of the viewport, in logical units.
    pub size: ReadSignal<Size>,
    /// Size of the surface, in physical pixels.
    pub physical_size: ReadSignal<PhysicalSize<u32>>,
    pub scale_factor: ReadSignal<f64>,
    /// Whether the window has keyboard focus.
    pub focused: ReadSignal<bool>,
    /// Whether the window is hidden from view, such as when minimized.
    pub occluded: ReadSignal<bool>,
    /// Whether a pointer is over the window.
    pub pointer_inside: ReadSignal<bool>,
    /// Notified when the user asks to close the window.
    pub close_requested: Trigger,
    /// Keep the window open when the user asks to close it.
    pub prevent_close: RwSignal<bool>,
    /// The focused node. Set it to move the focus, which runs the focus
    /// handlers on the next [`Window::poll`].
    pub focused_node: RwSignal<Option<NodeKey>>,
}

/// The writing ends of [`WindowState`], kept by the window.
struct StateWriters {
    size: WriteSignal<Size>,
    physical_size: WriteSignal<PhysicalSize<u32>>,
    scale_factor: WriteSignal<f64>,
    focused: WriteSignal<bool>,
    occluded: WriteSignal<bool>,
    pointer_inside: WriteSignal<bool>,
}

impl WindowState {
    fn new() -> (Self, StateWriters) {
        let (size, set_size) = signal(Size::ZERO);
        let (physical_size, set_physical_size) = signal(PhysicalSize::new(0, 0));
        let (scale_factor, set_scale_factor) = signal(1.0);
        let (focused, set_focused) = signal(false);
        let (occluded, set_occluded) = signal(false);
        let (pointer_inside, set_pointer_inside) = signal(false);

        (
            Self {
                size,
                physical_size,
                scale_factor,
                focused,
                occluded,
                pointer_inside,
                close_requested: Trigger::new(),
                prevent_close: RwSignal::new(false),
                focused_node: RwSignal::new(None),
            },
            StateWriters {
                size: set_size,
                physical_size: set_physical_size,
                scale_factor: set_scale_factor,
                focused: set_focused,
                occluded: set_occluded,
                pointer_inside: set_pointer_inside,
            },
        )
    }
}

/// The state of the window the current component is built in.
pub fn use_window() -> Option<WindowState> {
    use_context()
}

/// Set a signal only when the value changes, so followers are not woken
/// for nothing.
fn update<T: Clone + PartialEq + Send + Sync + 'static>(
    read: ReadSignal<T>,
    write: WriteSignal<T>,
    value: T,
) {
    if read.get_untracked() != value {
        write.set(value);
    }
}

/// A window showing a UI built with [`comet_reactivity`], independent of the
/// platform.
///
/// The platform layer feeds it input and window events, polls it after
/// each batch of events, and draws a frame when it
/// [needs one](Self::needs_redraw):
///
/// 1. [`layout`](Self::layout), which runs the frame listeners first,
/// 2. [`take_accessibility_update`](Self::take_accessibility_update) for the
///    platform's accessibility adapter,
/// 3. [`paint`](Self::paint) into the platform's scene.
pub struct Window {
    ui: UiRoot,
    pipeline: Pipeline,
    state: WindowState,
    writers: StateWriters,
    listeners: Listeners,
    /// Origin of frame times.
    start: Instant,
    physical_size: PhysicalSize<u32>,
    scale_factor: f64,
    /// Pointer position in logical units, the units of the layout.
    cursor: Option<Point>,
    /// The pointer of the last pointer event.
    pointer: Option<PointerInfo>,
    /// Nodes under the pointer, innermost first.
    hovered: Vec<NodeKey>,
    /// Node a pointer button was pressed on, clicked when released over it.
    pressed: Option<NodeKey>,
    /// Node capturing the pointer.
    capture: Option<NodeKey>,
    /// Focused node to scroll into view, waiting for its first layout.
    reveal: Option<NodeKey>,
    focus: Option<NodeKey>,
    /// Whether something not recorded in the tree needs a frame.
    redraw: bool,
    /// Owns the window's context and signals. Dropped after the UI.
    owner: Owner,
}

impl Window {
    /// Build the UI with the `root` component, under a new owner providing
    /// the window's [`WindowState`] as context. The node it returns is the
    /// root of the layout.
    pub fn new(root: impl FnOnce(&UiCx) -> NodeKey) -> Self {
        let owner = Owner::new();
        let (state, writers, listeners) = owner.with(|| {
            let (state, writers) = WindowState::new();
            let listeners = Listeners::new();
            provide_context(state);
            provide_context(listeners);

            (state, writers, listeners)
        });
        let ui = owner.with(|| UiRoot::new(root));

        Self {
            ui,
            pipeline: Pipeline::default(),
            state,
            writers,
            listeners,
            start: Instant::now(),
            physical_size: PhysicalSize::new(0, 0),
            scale_factor: 1.0,
            cursor: None,
            pointer: None,
            hovered: Vec::new(),
            pressed: None,
            capture: None,
            reveal: None,
            focus: None,
            redraw: true,
            owner,
        }
    }

    pub fn ui(&self) -> &UiRoot {
        &self.ui
    }

    /// The node built by the root component.
    pub fn root(&self) -> NodeKey {
        self.ui.root()
    }

    pub fn state(&self) -> WindowState {
        self.state
    }

    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    pub fn pipeline(&self) -> &Pipeline {
        &self.pipeline
    }

    pub fn pipeline_mut(&mut self) -> &mut Pipeline {
        &mut self.pipeline
    }

    pub fn focused(&self) -> Option<NodeKey> {
        self.focus
    }

    pub fn physical_size(&self) -> PhysicalSize<u32> {
        self.physical_size
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    /// Size of the viewport, in logical units: the surface divided by the
    /// scale factor.
    pub fn size(&self) -> Size {
        Size::new(
            f64::from(self.physical_size.width) / self.scale_factor,
            f64::from(self.physical_size.height) / self.scale_factor,
        )
    }

    // Window events.

    /// The surface was resized, or moved to a display with another scale
    /// factor.
    pub fn resize(&mut self, physical_size: PhysicalSize<u32>, scale_factor: f64) {
        self.physical_size = physical_size;
        self.scale_factor = scale_factor;

        let (state, writers) = (&self.state, &self.writers);
        update(state.physical_size, writers.physical_size, physical_size);
        update(state.scale_factor, writers.scale_factor, scale_factor);
        update(state.size, writers.size, self.size());
        self.redraw = true;
    }

    /// The window gained or lost keyboard focus.
    pub fn set_focused(&mut self, focused: bool) {
        update(self.state.focused, self.writers.focused, focused);
    }

    pub fn set_occluded(&mut self, occluded: bool) {
        update(self.state.occluded, self.writers.occluded, occluded);
        if !occluded {
            self.redraw = true;
        }
    }

    /// The user asked to close the window. Returns whether to close it,
    /// unless [`WindowState::prevent_close`] is set.
    pub fn request_close(&mut self) -> bool {
        self.state.close_requested.notify();

        !self.state.prevent_close.get_untracked()
    }

    // Frames.

    /// Run the bindings woken by the events so far, and apply focus changes
    /// made through [`WindowState::focused_node`]. Call it after each batch
    /// of events, without borrows of the tree.
    pub fn poll(&mut self) {
        self.ui.poll();

        let wanted = self.state.focused_node.get_untracked();
        if wanted != self.focus {
            self.set_focus(wanted);
        }

        // Focused or captured nodes removed from the tree.
        if self.focus.is_some_and(|node| !self.attached(node)) {
            self.set_focus(None);
        }
        if self.capture.is_some_and(|node| !self.attached(node)) {
            self.capture = None;
        }

        self.ui.poll();
    }

    /// Whether a frame should be drawn: the tree changed, something
    /// animates, or the window changed.
    pub fn needs_redraw(&self) -> bool {
        self.redraw || !self.ui.tree().borrow().changes().is_empty() || self.listeners.animating()
    }

    /// Run the frame handlers, then lay out the UI at the current size.
    /// Returns whether there is a layout to paint.
    pub fn layout(&mut self) -> bool {
        self.redraw = false;
        // This frame shows every change so far. Changes are taken even
        // without a frame, so they do not ask for one again.
        self.pipeline.take_changes(&mut self.ui.tree().borrow_mut());

        if self.physical_size.width == 0 || self.physical_size.height == 0 {
            return false;
        }

        // Frame listeners set signals: run the bindings they woke before
        // laying out.
        self.listeners.run_frame(self.start.elapsed());
        self.ui.poll();

        let size = self.size();
        let viewport = comet_layout::style::geometry::Size::new(size.width, size.height);
        let laid_out = self
            .pipeline
            .layout(&mut self.ui.tree().borrow_mut(), self.ui.root(), viewport)
            .is_some();

        // A node focused before it was laid out is revealed now. The scroll
        // is painted in the next frame, which the change requests.
        if laid_out {
            self.reveal_focus();
        }

        laid_out
    }

    /// Paint the last layout into `scene`, in physical pixels.
    pub fn paint(&mut self, scene: &mut impl anyrender::PaintScene) {
        let physical = kurbo::Rect::new(
            0.0,
            0.0,
            f64::from(self.physical_size.width),
            f64::from(self.physical_size.height),
        );
        let tree = self.ui.tree().borrow();

        self.pipeline
            .paint(&tree.ui, physical, self.scale_factor, scene);
    }

    // Accessibility.

    /// Start or stop building the accessibility tree, such as when an
    /// assistive technology connects. Starting builds the whole tree with
    /// the next frame, which it requests.
    pub fn set_accessibility(&mut self, active: bool) {
        self.pipeline.set_accessibility(active);
        self.redraw |= active;
    }

    /// The accessibility changes since the last update, with the focus.
    /// `None` while inactive or without changes.
    pub fn take_accessibility_update(&mut self) -> Option<TreeUpdate> {
        let tree = self.ui.tree().borrow();

        self.pipeline
            .take_accessibility_update(&tree.ui, self.scale_factor, self.focus)
    }

    /// Perform an action requested by an assistive technology.
    pub fn accessibility_action(&mut self, request: &ActionRequest) {
        let Some(node) = self.pipeline.accessibility_target(request.target_node) else {
            return;
        };

        match request.action {
            Action::Click => self.click(node),
            Action::Focus => self.set_focus(Some(node)),
            Action::Blur if self.focus == Some(node) => self.set_focus(None),
            _ => {}
        }
    }

    // Input.

    /// Dispatch a pointer event to the node under the pointer, or the node
    /// capturing it, and its ancestors, between the capturing and bubbling
    /// window listeners, then perform its default action. Window listeners
    /// also get the events nodes do not: pointer enter, leave, cancel and
    /// gestures.
    pub fn pointer_event(&mut self, event: &PointerEvent) {
        match event {
            PointerEvent::Move(update) => {
                let position = update.current.logical_point();
                self.pointer = Some(update.pointer);
                self.cursor = Some(position);
                self.set_pointer_inside(true);
                self.update_hover();

                let (path, local) = self.target_path(position);
                self.dispatch_pointer::<OnPointerMove>(
                    &path,
                    Some(position),
                    local,
                    event,
                    Some(update),
                );
            }
            PointerEvent::Down(button) => {
                let position = button.state.logical_point();
                self.pointer = Some(button.pointer);
                self.cursor = Some(position);

                let (path, local) = self.target_path(position);
                let prevented = self.dispatch_pointer::<OnPointerDown>(
                    &path,
                    Some(position),
                    local,
                    event,
                    Some(button),
                );
                self.pressed = path.first().copied();

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
                }
            }
            PointerEvent::Up(button) => {
                let position = button.state.logical_point();
                self.pointer = Some(button.pointer);
                self.cursor = Some(position);

                let (path, local) = self.target_path(position);
                self.dispatch_pointer::<OnPointerUp>(
                    &path,
                    Some(position),
                    local,
                    event,
                    Some(button),
                );
                self.capture = None;

                // Clicked when released over the node it was pressed on.
                if let Some(pressed) = self.pressed.take()
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
                self.pointer = Some(*pointer);
                self.pressed = None;
                self.capture = None;
                self.window_pointer(event, None);
            }
            PointerEvent::Enter(pointer) => {
                self.pointer = Some(*pointer);
                self.set_pointer_inside(true);
                self.window_pointer(event, None);
            }
            PointerEvent::Leave(pointer) => {
                self.pointer = Some(*pointer);
                self.cursor = None;
                self.set_pointer_inside(false);
                self.update_hover();
                self.window_pointer(event, None);
            }
            PointerEvent::Scroll(scroll) => {
                let position = scroll.state.logical_point();
                self.pointer = Some(scroll.pointer);
                self.cursor = Some(position);

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
                self.pointer = Some(gesture.pointer);
                self.window_pointer(event, Some(gesture.state.logical_point()));
            }
        }
    }

    /// Dispatch a keyboard event to the focused node, or the root without
    /// focus, and its ancestors, between the capturing and bubbling window
    /// listeners, then perform its default action: Tab moves the focus.
    pub fn keyboard_event(&mut self, event: &KeyboardEvent) {
        let target = self
            .focus
            .filter(|&node| self.attached(node))
            .unwrap_or(self.ui.root());
        let path = self.ancestors(Some(target));

        let prevented = match event.state {
            KeyState::Down => self.dispatch_key::<OnKeyDown>(&path, event),
            KeyState::Up => self.dispatch_key::<OnKeyUp>(&path, event),
        };

        if !prevented && event.state == KeyState::Down && event.key == Key::Named(NamedKey::Tab) {
            self.move_focus(!event.modifiers.contains(Modifiers::SHIFT));
        }
    }

    /// Click `node` without a pointer, as by an assistive technology.
    fn click(&mut self, node: NodeKey) {
        let path = self.ancestors(Some(node));
        let click = ClickEvent {
            button: None,
            count: 1,
        };

        self.dispatch::<OnClick>(&path, None, None, &click);
    }

    fn set_pointer_inside(&mut self, inside: bool) {
        update(
            self.state.pointer_inside,
            self.writers.pointer_inside,
            inside,
        );
    }

    /// Dispatch a pointer event: the capturing window listeners get `raw`,
    /// the handlers `P` of `path` get `event`, then the bubbling window
    /// listeners get `raw`, until one stops propagation. Then apply what
    /// they requested. Returns whether one prevented the default action.
    fn dispatch_pointer<P: HandlerProp>(
        &mut self,
        path: &[NodeKey],
        position: Option<Point>,
        local: Option<Point>,
        raw: &PointerEvent,
        event: Option<&P::Event>,
    ) -> bool {
        // Listeners added while dispatching only get later events.
        let capture = self.listeners.pointer(Phase::Capture);
        let bubble = self.listeners.pointer(Phase::Bubble);
        let mut cx = EventCx::new(path.first().copied(), position, local);

        run_listeners(&capture, &mut cx, raw);
        if let Some(event) = event
            && !cx.is_stopped()
        {
            self.run_nodes::<P>(path, &mut cx, event);
        }
        if !cx.is_stopped() {
            run_listeners(&bubble, &mut cx, raw);
        }

        self.finish(cx)
    }

    /// Dispatch a pointer event only window listeners get.
    fn window_pointer(&mut self, event: &PointerEvent, position: Option<Point>) {
        self.dispatch_pointer::<OnPointerMove>(&[], position, None, event, None);
    }

    /// Dispatch a keyboard event to the capturing window listeners, the
    /// handlers `P` of `path`, then the bubbling window listeners, like
    /// [`dispatch_pointer`](Self::dispatch_pointer).
    fn dispatch_key<P: HandlerProp<Event = KeyboardEvent>>(
        &mut self,
        path: &[NodeKey],
        event: &KeyboardEvent,
    ) -> bool {
        let capture = self.listeners.keyboard(Phase::Capture);
        let bubble = self.listeners.keyboard(Phase::Bubble);
        let mut cx = EventCx::new(path.first().copied(), None, None);

        run_listeners(&capture, &mut cx, event);
        if !cx.is_stopped() {
            self.run_nodes::<P>(path, &mut cx, event);
        }
        if !cx.is_stopped() {
            run_listeners(&bubble, &mut cx, event);
        }

        self.finish(cx)
    }

    /// Dispatch an event only nodes get, such as a click or a focus change,
    /// to the handlers `P` of `path`. Returns whether one prevented the
    /// default action.
    fn dispatch<P: HandlerProp>(
        &mut self,
        path: &[NodeKey],
        position: Option<Point>,
        local: Option<Point>,
        event: &P::Event,
    ) -> bool {
        let mut cx = EventCx::new(path.first().copied(), position, local);
        self.run_nodes::<P>(path, &mut cx, event);

        self.finish(cx)
    }

    /// Call the handlers `P` of `path`, a node and its ancestors, innermost
    /// first, until one stops propagation. Handlers run without borrows of
    /// the tree.
    fn run_nodes<P: HandlerProp>(&self, path: &[NodeKey], cx: &mut EventCx, event: &P::Event) {
        let handlers: Vec<_> = {
            let tree = self.ui.tree().borrow();
            path.iter()
                .filter_map(|&node| Some((node, tree.ui.prop::<P>(node)?.handler())))
                .collect()
        };

        for (node, handler) in handlers {
            cx.set_current(Some(node));
            handler.call(cx, event);

            if cx.is_stopped() {
                break;
            }
        }
        cx.set_current(None);
    }

    /// Apply what the handlers of a dispatch requested. Returns whether one
    /// prevented the default action.
    fn finish(&mut self, mut cx: EventCx) -> bool {
        for request in core::mem::take(&mut cx.requests) {
            match request {
                Request::Focus(node) => self.set_focus(node),
                Request::CapturePointer(node) => self.capture = Some(node),
                Request::ReleasePointer => self.capture = None,
            }
        }

        cx.is_default_prevented()
    }

    /// Move the focus to `node`, calling the blur handler of the node losing
    /// it and the focus handler of the node gaining it. Nodes not in the
    /// tree clear the focus.
    fn set_focus(&mut self, node: Option<NodeKey>) {
        let node = node.filter(|&node| self.attached(node));
        if self.state.focused_node.get_untracked() != node {
            self.state.focused_node.set(node);
        }

        if self.focus == node {
            return;
        }

        let old = core::mem::replace(&mut self.focus, node);
        // The accessibility tree reports the focus.
        self.redraw |= self.pipeline.accessibility_active();
        self.reveal = node;
        self.reveal_focus();

        if let Some(old) = old {
            self.dispatch::<OnBlur>(&[old], None, None, &FocusEvent { related: node });
        }
        if let Some(node) = node {
            self.dispatch::<OnFocus>(&[node], None, None, &FocusEvent { related: old });
        }
    }

    /// Scroll the scroll containers holding the node waiting to be revealed,
    /// so that its border box is visible. Kept waiting until the node is
    /// laid out.
    fn reveal_focus(&mut self) {
        let Some(node) = self.reveal else {
            return;
        };
        if !self.attached(node) {
            self.reveal = None;
            return;
        }

        let scrolls = {
            let tree = self.ui.tree().borrow();
            reveal_scrolls(&tree.ui, &self.pipeline.trees, node)
        };
        let Some(scrolls) = scrolls else {
            return;
        };
        self.reveal = None;

        let mut tree = self.ui.tree().borrow_mut();
        for (container, offset) in scrolls {
            tree.set_prop(container, ScrollLeft(offset.x));
            tree.set_prop(container, ScrollTop(offset.y));
        }
    }

    /// Focus the next focusable node in tree order, or the previous one,
    /// wrapping around.
    fn move_focus(&mut self, forward: bool) {
        let order = self.focus_order();
        let len = order.len();
        if len == 0 {
            return;
        }

        let next = match self
            .focus
            .and_then(|focus| order.iter().position(|&node| node == focus))
        {
            Some(index) if forward => (index + 1) % len,
            Some(index) => (index + len - 1) % len,
            None if forward => 0,
            None => len - 1,
        };
        self.set_focus(Some(order[next]));
    }

    /// The focusable nodes, in tree order.
    fn focus_order(&self) -> Vec<NodeKey> {
        let tree = self.ui.tree().borrow();
        let ui = &tree.ui;
        let mut order = Vec::new();
        let mut pending = vec![self.ui.root()];

        while let Some(node) = pending.pop() {
            if ui
                .prop::<Focusable>(node)
                .is_some_and(|focusable| focusable.0)
            {
                order.push(node);
            }

            let children: Vec<_> = ui.children(node).collect();
            pending.extend(children.into_iter().rev());
        }

        order
    }

    /// Call the leave handlers of the nodes the pointer left, innermost
    /// first, then the enter handlers of those it entered, outermost first.
    fn update_hover(&mut self) {
        let path = self
            .cursor
            .map(|cursor| self.hit_path(cursor))
            .unwrap_or_default();
        let left: Vec<_> = self
            .hovered
            .iter()
            .copied()
            .filter(|node| !path.contains(node))
            .collect();
        let entered: Vec<_> = path
            .iter()
            .rev()
            .copied()
            .filter(|node| !self.hovered.contains(node))
            .collect();
        self.hovered = path;

        let Some(pointer) = self.pointer else {
            return;
        };
        for node in left {
            self.dispatch::<OnPointerLeave>(&[node], self.cursor, None, &pointer);
        }
        for node in entered {
            self.dispatch::<OnPointerEnter>(&[node], self.cursor, None, &pointer);
        }
    }

    /// Scroll the innermost scroll container of `path` that can still move
    /// by `delta`, clamped to how far its content overflows in the last
    /// layout. Scrolling down moves the content up. The scroll is painted in
    /// the next frame, which the change requests.
    fn scroll(&mut self, path: &[NodeKey], delta: ScrollDelta) {
        let delta = match delta {
            ScrollDelta::LineDelta(x, y) => {
                Vec2::new(f64::from(x) * WHEEL_LINE, f64::from(y) * WHEEL_LINE)
            }
            ScrollDelta::PageDelta(x, y) => {
                let size = self.size();
                Vec2::new(f64::from(x) * size.width, f64::from(y) * size.height)
            }
            ScrollDelta::PixelDelta(position) => {
                let position = position.to_logical::<f64>(self.scale_factor);
                Vec2::new(position.x, position.y)
            }
        };

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
        if let Some(capture) = self.capture.filter(|&node| self.attached(node)) {
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

    /// `node` and its ancestors, innermost first.
    fn ancestors(&self, node: Option<NodeKey>) -> Vec<NodeKey> {
        let tree = self.ui.tree().borrow();

        successors(node, |&node| tree.ui.parent(node)).collect()
    }

    /// Whether `node` is in the window's tree.
    fn attached(&self, node: NodeKey) -> bool {
        let root = self.ui.root();
        let tree = self.ui.tree().borrow();

        successors(Some(node), |&node| tree.ui.parent(node)).any(|node| node == root)
    }
}

/// Call window listeners in order, until one stops propagation.
fn run_listeners<E>(listeners: &[WindowListener<E>], cx: &mut EventCx, event: &E) {
    cx.set_current(None);

    for listener in listeners {
        listener.call(cx, event);

        if cx.is_stopped() {
            break;
        }
    }
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
