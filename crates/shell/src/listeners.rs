//! Window-wide input and frame listeners.
//!
//! A listener receives every event of its kind in the window, whatever node
//! it targets. Each is stored in its own [`StoredValue`] under the owner
//! current when it is registered, so it stops once that owner is cleaned
//! up, such as when its component is removed. The returned
//! [`WindowListener`] removes it earlier. Listeners run under that owner,
//! so they can read its context and register other listeners.
//!
//! Input listeners run around the dispatch to nodes: [`Phase::Capture`]
//! listeners before the target's handlers, [`Phase::Bubble`] listeners after
//! its ancestors' handlers, unless propagation was stopped. Frame listeners
//! run before each frame is laid out.

use core::{fmt, hash::Hash, time::Duration};
use std::{cell::RefCell, rc::Rc};

use reactive_graph::{
    effect::RenderEffect,
    owner::{LocalStorage, Owner, StoredValue, WeakOwner, use_context},
    traits::{Dispose, Get, IsDisposed, UpdateValue, WithValue},
    wrappers::read::Signal,
};

use crate::event::{EventCx, KeyboardEvent, PointerEvent};

/// When an input listener runs, relative to the handlers of the nodes the
/// event is dispatched to.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Before the target's handlers.
    Capture,
    /// After the handlers of the target and its ancestors.
    #[default]
    Bubble,
}

type ListenerFn<E, C> = Rc<RefCell<dyn FnMut(&mut C, &E)>>;

struct Listener<E: 'static, C: 'static> {
    phase: Phase,
    /// Shared with running calls, so the listener can be removed or its
    /// phase changed while it runs.
    handler: ListenerFn<E, C>,
    /// Owner it was registered under, current while it runs. Weak, as the
    /// owner holds the listener.
    owner: Option<WeakOwner>,
}

/// A registered window listener of events `E`, called with a context `C`.
///
/// It is `Copy`, `Send` and `Sync`, but the listener itself stays on the
/// thread that registered it. Handles are equal only to themselves.
pub struct WindowListener<E: 'static, C: 'static = EventCx>(
    StoredValue<Listener<E, C>, LocalStorage>,
);

/// A registered frame listener.
pub type FrameListener = WindowListener<FrameEvent, FrameCx>;

impl<E: 'static, C: 'static> WindowListener<E, C> {
    #[track_caller]
    fn new(phase: Phase, handler: impl FnMut(&mut C, &E) + 'static) -> Self {
        let handler: ListenerFn<E, C> = Rc::new(RefCell::new(handler));
        let owner = Owner::current().map(|owner| owner.downgrade());

        Self(StoredValue::new_local(Listener {
            phase,
            handler,
            owner,
        }))
    }

    /// Stop the listener. A running call finishes first.
    pub fn remove(self) {
        self.0.dispose();
    }

    /// Whether it still listens: neither removed nor cleaned up with its
    /// owner.
    pub fn is_active(&self) -> bool {
        !self.0.is_disposed()
    }

    /// `None` once removed.
    pub fn phase(&self) -> Option<Phase> {
        self.0.try_with_value(|listener| listener.phase)
    }

    /// Run in `phase` from the next event. Returns `false` once removed.
    pub fn set_phase(&self, phase: Phase) -> bool {
        self.0
            .try_update_value(|listener| listener.phase = phase)
            .is_some()
    }

    /// Call the listener under the owner it was registered under. Returns
    /// `false` if it was removed, or is already running further up the
    /// stack.
    pub(crate) fn call(&self, cx: &mut C, event: &E) -> bool {
        let Some((handler, owner)) = self
            .0
            .try_with_value(|listener| (listener.handler.clone(), listener.owner.clone()))
        else {
            return false;
        };
        let Ok(mut handler) = handler.try_borrow_mut() else {
            return false;
        };

        match owner.and_then(|owner| owner.upgrade()) {
            Some(owner) => owner.with(|| handler(cx, event)),
            None => handler(cx, event),
        }
        true
    }
}

impl<E, C> Clone for WindowListener<E, C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E, C> Copy for WindowListener<E, C> {}

impl<E, C> PartialEq for WindowListener<E, C> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<E, C> Eq for WindowListener<E, C> {}

impl<E, C> Hash for WindowListener<E, C> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<E, C> fmt::Debug for WindowListener<E, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("WindowListener").finish_non_exhaustive()
    }
}

/// A frame about to be laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameEvent {
    /// Time of the frame, since the window opened.
    pub time: Duration,
    /// Time since the previous frame, or zero for the first frame after the
    /// window stopped drawing frames for listeners.
    pub delta: Duration,
}

/// Context of a frame listener's call.
#[derive(Debug, Default)]
pub struct FrameCx {
    stop: bool,
    request_frame: bool,
}

impl FrameCx {
    /// Remove the running listener after this call.
    pub fn stop(&mut self) {
        self.stop = true;
    }

    /// Draw another frame, even if no frame listener remains.
    pub fn request_frame(&mut self) {
        self.request_frame = true;
    }
}

#[derive(Default)]
struct ListenerSet {
    pointer: Vec<WindowListener<PointerEvent>>,
    keyboard: Vec<WindowListener<KeyboardEvent>>,
    frame: Vec<FrameListener>,
    /// Whether a frame listener asked for another frame.
    frame_requested: bool,
    /// Time of the last frame drawn for listeners, while they keep asking
    /// for frames.
    last_frame: Option<Duration>,
}

/// The listeners of a window, provided as context to its components.
#[derive(Clone, Copy)]
pub(crate) struct Listeners(StoredValue<ListenerSet, LocalStorage>);

impl Listeners {
    pub fn new() -> Self {
        Self(StoredValue::new_local(ListenerSet::default()))
    }

    fn add<E: 'static, C: 'static>(
        self,
        list: fn(&mut ListenerSet) -> &mut Vec<WindowListener<E, C>>,
        listener: WindowListener<E, C>,
    ) -> WindowListener<E, C> {
        self.0.update_value(|set| list(set).push(listener));
        listener
    }

    /// The active listeners of `list` in `phase`, or all with `None`, in
    /// registration order. Removed ones are dropped from the list. Calls run
    /// on the copy, so listeners added while they run only get later
    /// events.
    fn active<E: 'static, C: 'static>(
        self,
        list: fn(&mut ListenerSet) -> &mut Vec<WindowListener<E, C>>,
        phase: Option<Phase>,
    ) -> Vec<WindowListener<E, C>> {
        self.0
            .try_update_value(|set| {
                let list = list(set);
                list.retain(WindowListener::is_active);
                list.iter()
                    .copied()
                    .filter(|listener| phase.is_none() || listener.phase() == phase)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn pointer(self, phase: Phase) -> Vec<WindowListener<PointerEvent>> {
        self.active(|set| &mut set.pointer, Some(phase))
    }

    pub fn keyboard(self, phase: Phase) -> Vec<WindowListener<KeyboardEvent>> {
        self.active(|set| &mut set.keyboard, Some(phase))
    }

    /// Whether a frame should be drawn for the listeners.
    pub fn animating(self) -> bool {
        self.0
            .try_with_value(|set| {
                set.frame_requested || set.frame.iter().any(WindowListener::is_active)
            })
            .unwrap_or(false)
    }

    /// Run the frame listeners for a frame at `time`, since the window
    /// opened.
    pub fn run_frame(self, time: Duration) {
        let listeners = self.active(|set| &mut set.frame, None);
        let Some((requested, last)) = self.0.try_update_value(|set| {
            (
                core::mem::take(&mut set.frame_requested),
                set.last_frame,
            )
        }) else {
            return;
        };

        if listeners.is_empty() && !requested {
            self.0.update_value(|set| set.last_frame = None);
            return;
        }

        let event = FrameEvent {
            time,
            delta: last.map_or(Duration::ZERO, |last| time.saturating_sub(last)),
        };
        let mut request_frame = false;

        for listener in listeners {
            let mut cx = FrameCx::default();
            listener.call(&mut cx, &event);
            request_frame |= cx.request_frame;

            if cx.stop {
                listener.remove();
            }
        }

        self.0.update_value(|set| set.frame_requested |= request_frame);
        // Frames continue from this one only if listeners still ask for
        // them; otherwise the next one starts after an idle time.
        let animating = self.animating();
        self.0
            .update_value(|set| set.last_frame = animating.then_some(time));
    }
}

/// Listen to every pointer event of the current window, in `phase`.
///
/// Stored under the current owner. `None` outside a window.
#[track_caller]
pub fn on_window_pointer(
    phase: Phase,
    handler: impl FnMut(&mut EventCx, &PointerEvent) + 'static,
) -> Option<WindowListener<PointerEvent>> {
    let listeners = use_context::<Listeners>()?;

    Some(listeners.add(|set| &mut set.pointer, WindowListener::new(phase, handler)))
}

/// Listen to every keyboard event of the current window, in `phase`.
///
/// Stored under the current owner. `None` outside a window.
#[track_caller]
pub fn on_window_key(
    phase: Phase,
    handler: impl FnMut(&mut EventCx, &KeyboardEvent) + 'static,
) -> Option<WindowListener<KeyboardEvent>> {
    let listeners = use_context::<Listeners>()?;

    Some(listeners.add(|set| &mut set.keyboard, WindowListener::new(phase, handler)))
}

/// Call `handler` before every frame of the current window. Frames are drawn
/// continuously while it listens.
///
/// Stored under the current owner. `None` outside a window.
#[track_caller]
pub fn on_frame(handler: impl FnMut(&mut FrameCx, &FrameEvent) + 'static) -> Option<FrameListener> {
    let listeners = use_context::<Listeners>()?;

    Some(listeners.add(|set| &mut set.frame, WindowListener::new(Phase::Bubble, handler)))
}

/// Call `handler` once, before the next frame of the current window, which
/// it requests.
///
/// Stored under the current owner. `None` outside a window.
#[track_caller]
pub fn on_next_frame(handler: impl FnOnce(&FrameEvent) + 'static) -> Option<FrameListener> {
    let mut handler = Some(handler);

    on_frame(move |cx, event| {
        if let Some(handler) = handler.take() {
            handler(event);
        }
        cx.stop();
    })
}

/// Like [`on_frame`], but only while `active` is true, so frames stop once
/// nothing animates.
pub fn on_frame_while(
    active: impl Into<Signal<bool>>,
    handler: impl FnMut(&mut FrameCx, &FrameEvent) + 'static,
) {
    let active = active.into();
    let handler = Rc::new(RefCell::new(handler));

    // Each run cleans up the listener registered by the previous one, which
    // belongs to the effect's owner.
    let effect = RenderEffect::new(move |_| {
        if active.get() {
            let handler = handler.clone();
            on_frame(move |cx, event| (handler.borrow_mut())(cx, event));
        }
    });
    // Kept until the owner is cleaned up.
    StoredValue::new_local(effect);
}
