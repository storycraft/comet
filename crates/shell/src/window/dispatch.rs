//! Dispatching events to window listeners and node handlers.

use comet_tree::node::NodeKey;
use kurbo::Point;

use super::Window;
use crate::{
    event::{ClickEvent, EventCx, KeyboardEvent, PointerEvent, Request},
    listeners::{Phase, WindowListener},
    style::{HandlerProp, OnClick, OnPointerMove},
};

impl Window {
    /// Dispatch a pointer event: the capturing window listeners get `raw`,
    /// the handlers `P` of `path` get `event`, then the bubbling window
    /// listeners get `raw`, until one stops propagation. Then apply what
    /// they requested. Returns whether one prevented the default action.
    pub(super) fn dispatch_pointer<P: HandlerProp>(
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
    pub(super) fn window_pointer(&mut self, event: &PointerEvent, position: Option<Point>) {
        self.dispatch_pointer::<OnPointerMove>(&[], position, None, event, None);
    }

    /// Dispatch a keyboard event to the capturing window listeners, the
    /// handlers `P` of `path`, then the bubbling window listeners, like
    /// [`dispatch_pointer`](Self::dispatch_pointer).
    pub(super) fn dispatch_key<P: HandlerProp<Event = KeyboardEvent>>(
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
    pub(super) fn dispatch<P: HandlerProp>(
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

    /// Click `node` without a pointer, as by an assistive technology.
    pub(super) fn click(&mut self, node: NodeKey) {
        let path = self.ancestors(Some(node));
        let click = ClickEvent {
            button: None,
            count: 1,
        };

        self.dispatch::<OnClick>(&path, None, None, &click);
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
                Request::CapturePointer(node) => self.pointer.capture = Some(node),
                Request::ReleasePointer => self.pointer.capture = None,
            }
        }

        cx.is_default_prevented()
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
