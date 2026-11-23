//! Input event handlers and their context.
//!
//! Handlers are registered as [style props](crate::styles) holding a
//! [`Handler`]: a copyable handle to a closure stored under the current
//! reactive owner, dropped once that owner is cleaned up. Handlers run on
//! the event loop thread, without borrows of the tree, so they may set
//! signals and build nodes.

use core::fmt;

use comet_tree::node::NodeKey;
use kurbo::Point;
use reactive_graph::{
    owner::{LocalStorage, StoredValue},
    traits::{IsDisposed, UpdateValue},
};
use ui_events::pointer::PointerButton;

pub use ui_events::{
    ScrollDelta,
    keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
    pointer::{PointerButtonEvent, PointerEvent, PointerInfo, PointerScrollEvent, PointerUpdate},
};

type HandlerFn<E> = dyn FnMut(&mut EventCx, &E);

/// A handler of events `E`, stored under the owner current when it was
/// created and dropped with it.
///
/// It is `Copy`, `Send` and `Sync`, so it can be held by style props, but
/// the closure itself stays on the thread that created it. Handles are
/// equal only to themselves.
pub struct Handler<E: 'static>(StoredValue<Box<HandlerFn<E>>, LocalStorage>);

impl<E: 'static> Handler<E> {
    /// Store `handler` under the current owner.
    #[track_caller]
    pub fn new(handler: impl FnMut(&mut EventCx, &E) + 'static) -> Self {
        Self(StoredValue::new_local(Box::new(handler)))
    }

    /// Whether its owner was cleaned up, so it never runs again.
    pub fn is_disposed(&self) -> bool {
        self.0.is_disposed()
    }

    /// Run the handler. Returns `false` if it is disposed, or already
    /// running further up the stack.
    pub(crate) fn call(&self, cx: &mut EventCx, event: &E) -> bool {
        self.0
            .try_update_value(|handler| handler(cx, event))
            .is_some()
    }
}

impl<E> Clone for Handler<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E> Copy for Handler<E> {}

impl<E> PartialEq for Handler<E> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<E> Eq for Handler<E> {}

impl<E> fmt::Debug for Handler<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Handler").finish_non_exhaustive()
    }
}

/// A click: a press and release of a pointer button over the same node, or
/// an activation from an assistive technology.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickEvent {
    /// `None` for activations without a pointer.
    pub button: Option<PointerButton>,
    /// Clicks in a row, from 1.
    pub count: u8,
}

/// A node gaining or losing focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusEvent {
    /// The node losing focus for a focus event, or gaining it for a blur.
    pub related: Option<NodeKey>,
}

/// Something a handler asked of the window, applied once dispatch is done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Request {
    Focus(Option<NodeKey>),
    CapturePointer(NodeKey),
    ReleasePointer,
}

/// Context of an event being dispatched to its handlers.
#[derive(Debug)]
pub struct EventCx {
    target: NodeKey,
    current: NodeKey,
    position: Option<Point>,
    local_position: Option<Point>,
    stopped: bool,
    default_prevented: bool,
    pub(crate) requests: Vec<Request>,
}

impl EventCx {
    pub(crate) fn new(target: NodeKey, position: Option<Point>, local: Option<Point>) -> Self {
        Self {
            target,
            current: target,
            position,
            local_position: local,
            stopped: false,
            default_prevented: false,
            requests: Vec::new(),
        }
    }

    pub(crate) fn set_current(&mut self, node: NodeKey) {
        self.current = node;
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// The node the event was dispatched to.
    pub fn target(&self) -> NodeKey {
        self.target
    }

    /// The node whose handler is running: the target or one of its
    /// ancestors.
    pub fn current_target(&self) -> NodeKey {
        self.current
    }

    /// Pointer position in the window, in logical units. `None` for events
    /// without a pointer.
    pub fn position(&self) -> Option<Point> {
        self.position
    }

    /// Pointer position in the local coordinates of the fragment hit in
    /// [`target`](Self::target), with its border box's top-left corner at
    /// the origin. `None` when the target was not hit, such as for captured
    /// pointers or keyboard events.
    pub fn local_position(&self) -> Option<Point> {
        self.local_position
    }

    /// Do not call the handlers of the remaining ancestors.
    pub fn stop_propagation(&mut self) {
        self.stopped = true;
    }

    /// Skip the window's default action, such as focusing on press or
    /// scrolling on wheel.
    pub fn prevent_default(&mut self) {
        self.default_prevented = true;
    }

    pub fn is_default_prevented(&self) -> bool {
        self.default_prevented
    }

    /// Focus `node`, or clear the focus with `None`, once dispatch is done.
    pub fn request_focus(&mut self, node: Option<NodeKey>) {
        self.requests.push(Request::Focus(node));
    }

    /// Send the pointer's events to the current target until it is
    /// released, wherever it moves.
    pub fn capture_pointer(&mut self) {
        self.requests.push(Request::CapturePointer(self.current));
    }

    pub fn release_pointer(&mut self) {
        self.requests.push(Request::ReleasePointer);
    }
}
