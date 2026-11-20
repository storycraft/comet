//! Input and frame handlers.
//!
//! Handlers belong to the owner they are registered under, and are removed
//! when it is cleaned up. They run on the event loop thread, without borrows
//! of the tree, so they may set signals and register handlers.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    time::Duration,
};

use comet_reactivity::NodeBuild;
use comet_tree::node::NodeKey;
use reactive_graph::{
    effect::RenderEffect, owner::on_cleanup, traits::Get, wrappers::read::Signal,
};

thread_local! {
    static HANDLERS: RefCell<Handlers> = RefCell::default();
}

type ClickHandler = Rc<dyn Fn()>;
type HoverHandler = Rc<dyn Fn(bool)>;

/// Handlers by node, each with the id it was registered with, so a cleanup
/// only removes its own handler.
#[derive(Default)]
struct Handlers {
    next_id: u64,
    click: HashMap<NodeKey, (u64, ClickHandler)>,
    hover: HashMap<NodeKey, (u64, HoverHandler)>,
    frame: Vec<(u64, Rc<FrameHandler>)>,
}

impl Handlers {
    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}

struct FrameHandler {
    /// Time of its last frame, since the window opened.
    last: Cell<Option<Duration>>,
    handler: Box<dyn Fn(Duration)>,
}

/// Input handlers of a node being built.
pub trait Interactive: Sized {
    /// Call `handler` when the node or one of its descendants is clicked,
    /// replacing an earlier click handler.
    fn on_click(self, handler: impl Fn() + 'static) -> Self;

    /// Call `handler` when the cursor enters (`true`) or leaves (`false`) the
    /// node and its descendants, replacing an earlier hover handler.
    fn on_hover(self, handler: impl Fn(bool) + 'static) -> Self;
}

impl Interactive for NodeBuild<'_> {
    fn on_click(self, handler: impl Fn() + 'static) -> Self {
        let node = self.key();
        let id = HANDLERS.with_borrow_mut(|handlers| {
            let id = handlers.id();
            handlers.click.insert(node, (id, Rc::new(handler)));
            id
        });

        self.cx().owner().with(|| {
            on_cleanup(move || {
                remove(|handlers| {
                    if handlers
                        .click
                        .get(&node)
                        .is_some_and(|(other, _)| *other == id)
                    {
                        handlers.click.remove(&node);
                    }
                })
            })
        });
        self
    }

    fn on_hover(self, handler: impl Fn(bool) + 'static) -> Self {
        let node = self.key();
        let id = HANDLERS.with_borrow_mut(|handlers| {
            let id = handlers.id();
            handlers.hover.insert(node, (id, Rc::new(handler)));
            id
        });

        self.cx().owner().with(|| {
            on_cleanup(move || {
                remove(|handlers| {
                    if handlers
                        .hover
                        .get(&node)
                        .is_some_and(|(other, _)| *other == id)
                    {
                        handlers.hover.remove(&node);
                    }
                })
            })
        });
        self
    }
}

/// Call `handler` before every frame with the time since its previous frame,
/// zero on the first. Frames are drawn continuously while a frame handler is
/// registered.
///
/// Registered under the current owner, if any.
pub fn on_frame(handler: impl Fn(Duration) + 'static) {
    let handler = Rc::new(FrameHandler {
        last: Cell::new(None),
        handler: Box::new(handler),
    });
    let id = HANDLERS.with_borrow_mut(|handlers| {
        let id = handlers.id();
        handlers.frame.push((id, handler));
        id
    });

    on_cleanup(move || remove(|handlers| handlers.frame.retain(|(other, _)| *other != id)));
}

/// Like [`on_frame`], but only while `active` is true, so frames stop once
/// nothing animates. The time restarts from zero whenever it turns true.
pub fn on_frame_while(active: impl Into<Signal<bool>>, handler: impl Fn(Duration) + 'static) {
    let active = active.into();
    let handler = Rc::new(handler);

    // Each run cleans up the handler registered by the previous one.
    let effect = RenderEffect::new(move |_| {
        if active.get() {
            let handler = handler.clone();
            on_frame(move |delta| handler(delta));
        }
    });
    on_cleanup(move || drop(effect));
}

/// Remove a handler. Cleanups may run on another thread, which has no
/// handlers to remove.
fn remove(f: impl FnOnce(&mut Handlers)) {
    let _ = HANDLERS.try_with(|handlers| {
        if let Ok(mut handlers) = handlers.try_borrow_mut() {
            f(&mut handlers);
        }
    });
}

/// The click handlers of `path`, innermost first.
pub(crate) fn click_handlers(path: &[NodeKey]) -> Vec<ClickHandler> {
    HANDLERS.with_borrow(|handlers| {
        path.iter()
            .filter_map(|node| handlers.click.get(node).map(|(_, handler)| handler.clone()))
            .collect()
    })
}

/// The nodes of `path` with hover handlers.
pub(crate) fn hover_targets(path: &[NodeKey]) -> Vec<NodeKey> {
    HANDLERS.with_borrow(|handlers| {
        path.iter()
            .copied()
            .filter(|node| handlers.hover.contains_key(node))
            .collect()
    })
}

pub(crate) fn hover_handler(node: NodeKey) -> Option<HoverHandler> {
    HANDLERS.with_borrow(|handlers| {
        handlers
            .hover
            .get(&node)
            .map(|(_, handler)| handler.clone())
    })
}

/// Whether a frame handler is registered.
pub(crate) fn animating() -> bool {
    HANDLERS.with_borrow(|handlers| !handlers.frame.is_empty())
}

/// Run the frame handlers for a frame at `now`, the time since the window
/// opened.
pub(crate) fn run_frame(now: Duration) {
    let frame: Vec<_> = HANDLERS.with_borrow(|handlers| {
        handlers
            .frame
            .iter()
            .map(|(_, handler)| handler.clone())
            .collect()
    });

    for handler in frame {
        let last = handler.last.replace(Some(now)).unwrap_or(now);
        (handler.handler)(now.saturating_sub(last));
    }
}
