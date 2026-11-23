//! Window-wide input and frame listeners of a headless window.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    thread,
    time::Duration,
};

use comet_layout::style::unit::*;
use comet_reactivity::{Styled, UiCx};
use comet_shell::{
    Handler, Phase, Window, WindowListener, on_frame, on_next_frame, on_window_key,
    on_window_pointer,
    style::{Focusable, OnKeyDown, OnPointerDown},
    ui_events::{
        keyboard::{Code, Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
        pointer::{
            PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerState,
            PointerType,
        },
    },
};
use comet_tree::node::NodeKey;
use dpi::{PhysicalPosition, PhysicalSize};
use reactive_graph::owner::Owner;

type Log = Rc<RefCell<Vec<String>>>;

const MOUSE: PointerInfo = PointerInfo {
    pointer_id: Some(PointerId::PRIMARY),
    persistent_device_id: None,
    pointer_type: PointerType::Mouse,
};

fn down(x: f64, y: f64) -> PointerEvent {
    PointerEvent::Down(PointerButtonEvent {
        button: Some(PointerButton::Primary),
        pointer: MOUSE,
        state: PointerState {
            position: PhysicalPosition::new(x, y),
            count: 1,
            ..Default::default()
        },
    })
}

fn key(key: Key) -> KeyboardEvent {
    KeyboardEvent {
        state: KeyState::Down,
        key,
        code: Code::Unidentified,
        modifiers: Modifiers::empty(),
        ..Default::default()
    }
}

fn push(log: &Log, entry: impl Into<String>) {
    log.borrow_mut().push(entry.into());
}

fn open(window: &mut Window) {
    window.resize(PhysicalSize::new(400, 300), 1.0);
    window.poll();
    window.layout();
}

/// A window with one 100×100 box at the top-left, built by `setup` along
/// with the listeners it registers. Returns the box's key.
fn window_with(
    log: &Log,
    stop_at_node: bool,
    setup: impl FnOnce(&Log) + 'static,
) -> (Window, Rc<Cell<Option<NodeKey>>>) {
    let node = Rc::new(Cell::new(None));
    let mut window = Window::new({
        let log = log.clone();
        let node = node.clone();
        move |cx: &UiCx| {
            setup(&log);
            let key = cx
                .div()
                .size(size(px(100), px(100)))
                .style((
                    Focusable(true),
                    OnPointerDown(Handler::new({
                        let log = log.clone();
                        move |cx, _| {
                            push(&log, "node");
                            if stop_at_node {
                                cx.stop_propagation();
                            }
                        }
                    })),
                    OnKeyDown(Handler::new({
                        let log = log.clone();
                        move |_, _| push(&log, "node key")
                    })),
                ))
                .build();
            node.set(Some(key));
            key
        }
    });
    open(&mut window);

    (window, node)
}

#[test]
fn capture_runs_before_nodes_and_bubble_after() {
    let log = Log::default();
    let (mut window, node) = window_with(&log, false, |log| {
        let capture = log.clone();
        on_window_pointer(Phase::Capture, move |cx, _| {
            assert_eq!(cx.current_target(), None, "no node runs a window listener");
            push(&capture, "capture");
        });
        let bubble = log.clone();
        on_window_pointer(Phase::Bubble, move |cx, _| {
            assert!(cx.target().is_some(), "the target is the node hit");
            push(&bubble, "bubble");
        });
    });

    window.pointer_event(&down(10.0, 10.0));

    assert_eq!(*log.borrow(), ["capture", "node", "bubble"]);
    assert!(node.get().is_some());
}

#[test]
fn stopping_in_capture_skips_nodes_and_bubble() {
    let log = Log::default();
    let (mut window, _) = window_with(&log, false, |log| {
        let capture = log.clone();
        on_window_pointer(Phase::Capture, move |cx, _| {
            push(&capture, "capture");
            cx.stop_propagation();
        });
        let bubble = log.clone();
        on_window_pointer(Phase::Bubble, move |_, _| push(&bubble, "bubble"));
    });

    window.pointer_event(&down(10.0, 10.0));

    assert_eq!(*log.borrow(), ["capture"]);
}

#[test]
fn stopping_at_a_node_skips_bubble() {
    let log = Log::default();
    let (mut window, _) = window_with(&log, true, |log| {
        let bubble = log.clone();
        on_window_pointer(Phase::Bubble, move |_, _| push(&bubble, "bubble"));
    });

    window.pointer_event(&down(10.0, 10.0));

    assert_eq!(*log.borrow(), ["node"]);
}

#[test]
fn preventing_tab_in_capture_keeps_the_focus() {
    let log = Log::default();
    let (mut window, _) = window_with(&log, false, |_| {
        on_window_key(Phase::Capture, |cx, event| {
            if event.key == Key::Named(NamedKey::Tab) {
                cx.prevent_default();
            }
        });
    });

    window.keyboard_event(&key(Key::Named(NamedKey::Tab)));

    assert_eq!(window.focused(), None, "Tab did not move the focus");
}

#[test]
fn listeners_get_events_nodes_do_not() {
    let log = Log::default();
    let (mut window, _) = window_with(&log, false, |log| {
        let log = log.clone();
        on_window_pointer(Phase::Bubble, move |cx, event| {
            let kind = match event {
                PointerEvent::Down(_) => "down",
                PointerEvent::Leave(_) => "leave",
                _ => "other",
            };
            push(&log, format!("{kind} {:?}", cx.target().is_some()));
        });
    });

    // Outside the viewport, no node is hit.
    window.pointer_event(&down(1000.0, 1000.0));
    window.pointer_event(&PointerEvent::Leave(MOUSE));

    assert_eq!(*log.borrow(), ["down false", "leave false"]);
}

#[test]
fn remove_stops_a_listener_even_from_its_own_call() {
    let log = Log::default();
    let handle = Rc::new(Cell::new(None::<WindowListener<KeyboardEvent>>));
    let (mut window, _) = window_with(&log, false, {
        let handle = handle.clone();
        move |log| {
            let log = log.clone();
            let this = handle.clone();
            handle.set(on_window_key(Phase::Bubble, move |_, _| {
                push(&log, "listener");
                this.get().unwrap().remove();
            }));
        }
    });
    let listener = handle.get().expect("registered in a window");
    assert!(listener.is_active());

    window.keyboard_event(&key(Key::Character("a".into())));
    window.keyboard_event(&key(Key::Character("b".into())));

    // Keys also reach the box, the root, without focus.
    assert_eq!(
        log.borrow().iter().filter(|entry| *entry == "listener").count(),
        1,
        "called once"
    );
    assert!(!listener.is_active());
    assert_eq!(listener.phase(), None);
}

#[test]
fn set_phase_moves_a_listener() {
    let log = Log::default();
    let handle = Rc::new(Cell::new(None));
    let (mut window, _) = window_with(&log, true, {
        let handle = handle.clone();
        move |log| {
            let log = log.clone();
            handle.set(on_window_pointer(Phase::Bubble, move |_, _| {
                push(&log, "listener")
            }));
        }
    });
    let listener = handle.get().unwrap();

    // The node stops propagation before the bubble phase.
    window.pointer_event(&down(10.0, 10.0));
    assert!(listener.set_phase(Phase::Capture));
    window.pointer_event(&down(10.0, 10.0));

    assert_eq!(*log.borrow(), ["node", "listener", "node"]);
    assert_eq!(listener.phase(), Some(Phase::Capture));
}

#[test]
fn listeners_stop_with_their_owner() {
    let log = Log::default();
    let owner = Rc::new(RefCell::new(None::<Owner>));
    let handle = Rc::new(Cell::new(None));
    let (mut window, _) = window_with(&log, false, {
        let (owner, handle) = (owner.clone(), handle.clone());
        move |log| {
            let child = Owner::new();
            let log = log.clone();
            handle.set(child.with(|| {
                on_window_key(Phase::Bubble, move |_, _| push(&log, "listener"))
            }));
            *owner.borrow_mut() = Some(child);
        }
    });

    window.keyboard_event(&key(Key::Character("a".into())));
    owner.borrow().as_ref().unwrap().cleanup();
    window.keyboard_event(&key(Key::Character("b".into())));

    assert_eq!(
        log.borrow().iter().filter(|entry| *entry == "listener").count(),
        1,
        "not called once its owner was cleaned up"
    );
    let listener: WindowListener<KeyboardEvent> = handle.get().unwrap();
    assert!(!listener.is_active());
}

#[test]
fn listeners_added_while_dispatching_get_later_events() {
    let log = Log::default();
    let (mut window, _) = window_with(&log, false, |log| {
        let log = log.clone();
        let added = Cell::new(false);
        on_window_key(Phase::Bubble, move |_, _| {
            push(&log, "first");
            // Runs under its owner, so it finds the window.
            if !added.replace(true) {
                let log = log.clone();
                assert!(
                    on_window_key(Phase::Bubble, move |_, _| push(&log, "second")).is_some(),
                    "registered from a listener"
                );
            }
        });
    });

    window.keyboard_event(&key(Key::Character("a".into())));
    window.keyboard_event(&key(Key::Character("b".into())));

    let listeners: Vec<_> = log
        .borrow()
        .iter()
        .filter(|entry| *entry != "node key")
        .cloned()
        .collect();
    assert_eq!(listeners, ["first", "first", "second"]);
}

#[test]
fn frame_delta_is_zero_first_then_the_frame_interval() {
    let deltas = Rc::new(RefCell::new(Vec::new()));
    let mut window = Window::new({
        let deltas = deltas.clone();
        move |cx: &UiCx| {
            on_frame(move |_, frame| deltas.borrow_mut().push(frame.delta));
            cx.div().build()
        }
    });
    open(&mut window);
    thread::sleep(Duration::from_millis(5));
    assert!(window.needs_redraw(), "animating");
    window.layout();

    let deltas = deltas.borrow();
    assert_eq!(deltas.len(), 2, "one call per frame");
    assert_eq!(deltas[0], Duration::ZERO, "zero on the first frame");
    assert!(deltas[1] >= Duration::from_millis(5), "the time between frames");
}

#[test]
fn stop_ends_the_frames() {
    let calls = Rc::new(Cell::new(0));
    let mut window = Window::new({
        let calls = calls.clone();
        move |cx: &UiCx| {
            on_frame(move |cx, _| {
                calls.set(calls.get() + 1);
                if calls.get() == 2 {
                    cx.stop();
                }
            });
            cx.div().build()
        }
    });
    open(&mut window);
    window.layout();

    assert_eq!(calls.get(), 2);
    assert!(!window.needs_redraw(), "no frame without listeners");
    window.layout();
    assert_eq!(calls.get(), 2, "not called after stopping");
}

#[test]
fn next_frame_runs_once_and_request_frame_asks_for_one_more() {
    let log = Log::default();
    let mut window = Window::new({
        let log = log.clone();
        move |cx: &UiCx| {
            let next = log.clone();
            on_next_frame(move |_| push(&next, "next"));
            let request = log.clone();
            on_frame(move |cx, _| {
                push(&request, "request");
                cx.request_frame();
                cx.stop();
            });
            cx.div().build()
        }
    });
    open(&mut window);

    assert_eq!(*log.borrow(), ["next", "request"]);
    assert!(window.needs_redraw(), "the requested frame");
    window.layout();
    assert!(!window.needs_redraw(), "only one frame was requested");
    assert_eq!(log.borrow().len(), 2, "neither runs again");
}

#[test]
fn frame_delta_restarts_after_idle() {
    let delta = Rc::new(Cell::new(None));
    let mut window = Window::new(|cx: &UiCx| {
        on_next_frame(|_| {});
        cx.div().build()
    });
    open(&mut window);
    assert!(!window.needs_redraw(), "idle");
    thread::sleep(Duration::from_millis(5));

    window.owner().with({
        let delta = delta.clone();
        || on_next_frame(move |frame| delta.set(Some(frame.delta)))
    });
    assert!(window.needs_redraw(), "a new listener asks for a frame");
    window.layout();

    assert_eq!(delta.get(), Some(Duration::ZERO), "no jump over the idle time");
}
