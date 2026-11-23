//! Frame handlers of a window.
//!
//! Handlers are stored under the owner current when they are registered,
//! and stop once it is cleaned up. They belong to the window found in the
//! reactive context.

use std::{cell::RefCell, rc::Rc, time::Duration};

use reactive_graph::{
    effect::RenderEffect,
    owner::{LocalStorage, StoredValue, use_context},
    traits::{Get, IsDisposed, UpdateValue, WithValue},
    wrappers::read::Signal,
};

type FrameFn = Box<dyn FnMut(Duration)>;

struct FrameEntry {
    handler: StoredValue<FrameFn, LocalStorage>,
    /// Time of its last frame, since the window opened.
    last: Option<Duration>,
}

/// Frame handlers of a window, provided as context to its components.
#[derive(Clone, Copy)]
pub(crate) struct Frames(StoredValue<Vec<FrameEntry>, LocalStorage>);

impl Frames {
    pub fn new() -> Self {
        Self(StoredValue::new_local(Vec::new()))
    }

    /// Whether a handler is registered and not disposed.
    pub fn animating(self) -> bool {
        self.0
            .try_with_value(|entries| entries.iter().any(|entry| !entry.handler.is_disposed()))
            .unwrap_or(false)
    }

    /// Run the handlers for a frame at `now`, the time since the window
    /// opened, dropping disposed ones. Handlers run without borrows, so they
    /// may register other handlers, which run from the next frame.
    pub fn run(self, now: Duration) {
        let mut due = Vec::new();
        self.0.update_value(|entries| {
            entries.retain(|entry| !entry.handler.is_disposed());
            for entry in entries {
                let last = entry.last.replace(now).unwrap_or(now);
                due.push((entry.handler, now.saturating_sub(last)));
            }
        });

        for (handler, delta) in due {
            handler.try_update_value(|handler| handler(delta));
        }
    }
}

/// Call `handler` before every frame of the current window with the time
/// since its previous frame, zero on the first. Frames are drawn
/// continuously while a frame handler is registered.
///
/// Registered under the current owner. Does nothing outside a window.
#[track_caller]
pub fn on_frame(handler: impl FnMut(Duration) + 'static) {
    let Some(frames) = use_context::<Frames>() else {
        return;
    };
    let handler = StoredValue::new_local(Box::new(handler) as FrameFn);

    frames.0.update_value(|entries| {
        entries.push(FrameEntry {
            handler,
            last: None,
        })
    });
}

/// Like [`on_frame`], but only while `active` is true, so frames stop once
/// nothing animates. The time restarts from zero whenever it turns true.
pub fn on_frame_while(active: impl Into<Signal<bool>>, handler: impl FnMut(Duration) + 'static) {
    let active = active.into();
    let handler = Rc::new(RefCell::new(handler));

    // Each run cleans up the handler registered by the previous one.
    let effect = RenderEffect::new(move |_| {
        if active.get() {
            let handler = handler.clone();
            on_frame(move |delta| (handler.borrow_mut())(delta));
        }
    });
    // Kept until the owner is cleaned up.
    StoredValue::new_local(effect);
}
