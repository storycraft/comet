//! Input dispatch to nodes, focus and scrolling of a headless window, with
//! handlers set through the `InputStyled` helpers.

use std::{cell::RefCell, rc::Rc};

use comet_layout::style::{geometry::Point as StylePoint, taffy::Overflow, unit::*};
use comet_reactivity::{Styled, UiCx};
use comet_shell::{
    Window,
    style::InputStyled,
    ui_events::{
        ScrollDelta,
        keyboard::{Code, Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
        pointer::{
            PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo,
            PointerScrollEvent, PointerState, PointerType, PointerUpdate,
        },
    },
};
use comet_tree::node::NodeKey;
use dpi::{PhysicalPosition, PhysicalSize};
use reactive_graph::traits::{GetUntracked, Set};

type Log = Rc<RefCell<Vec<String>>>;

const MOUSE: PointerInfo = PointerInfo {
    pointer_id: Some(PointerId::PRIMARY),
    persistent_device_id: None,
    pointer_type: PointerType::Mouse,
};

fn state(x: f64, y: f64) -> PointerState {
    PointerState {
        position: PhysicalPosition::new(x, y),
        count: 1,
        ..Default::default()
    }
}

fn down(x: f64, y: f64) -> PointerEvent {
    PointerEvent::Down(PointerButtonEvent {
        button: Some(PointerButton::Primary),
        pointer: MOUSE,
        state: state(x, y),
    })
}

fn up(x: f64, y: f64) -> PointerEvent {
    PointerEvent::Up(PointerButtonEvent {
        button: Some(PointerButton::Primary),
        pointer: MOUSE,
        state: state(x, y),
    })
}

fn moved(x: f64, y: f64) -> PointerEvent {
    PointerEvent::Move(PointerUpdate {
        pointer: MOUSE,
        current: state(x, y),
        coalesced: Vec::new(),
        predicted: Vec::new(),
    })
}

fn key_down(key: Key, modifiers: Modifiers) -> KeyboardEvent {
    KeyboardEvent {
        state: KeyState::Down,
        key,
        code: Code::Unidentified,
        modifiers,
        ..Default::default()
    }
}

fn logger(log: &Log, name: &'static str) -> impl Fn(&str) + Clone + 'static {
    let log = log.clone();
    move |event| log.borrow_mut().push(format!("{name}:{event}"))
}

struct Nodes {
    outer: NodeKey,
    inner: NodeKey,
    second: NodeKey,
}

/// `outer` (0,0)-(200,100) holds `inner` (0,0)-(100,50); `second` is below
/// at (0,100)-(200,200). All are focusable.
fn window(log: &Log) -> (Window, Rc<RefCell<Option<Nodes>>>) {
    let nodes = Rc::new(RefCell::new(None));
    let window = Window::new({
        let log = log.clone();
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let outer_log = logger(&log, "outer");
            let inner_log = logger(&log, "inner");
            let second_log = logger(&log, "second");
            let mut inner = None;

            let outer = cx
                .div()
                .size(size(px(200), px(100)))
                .focusable(true)
                .on_click({
                    let log = outer_log.clone();
                    move |_, _| log("click")
                })
                .on_key_down({
                    let log = outer_log.clone();
                    move |cx, _| log(&format!("key({:?})", cx.target() == cx.current_target()))
                })
                .on_focus({
                    let log = outer_log.clone();
                    move |_, _| log("focus")
                })
                .on_blur(move |_, _| outer_log("blur"))
                .children(|cx| {
                    let key = cx
                        .div()
                        .size(size(px(100), px(50)))
                        .focusable(true)
                        .on_click({
                            let log = inner_log.clone();
                            move |_, _| log("click")
                        })
                        .on_hover(move |entered| inner_log(if entered { "enter" } else { "leave" }))
                        .build();
                    inner = Some(key);
                    key
                });
            let second = cx
                .div()
                .size(size(px(200), px(100)))
                .focusable(true)
                .on_focus(move |_, _| second_log("focus"))
                .build();

            *nodes.borrow_mut() = Some(Nodes {
                outer,
                inner: inner.unwrap(),
                second,
            });
            cx.div().children(|_| [outer, second])
        }
    });

    (window, nodes)
}

fn open(window: &mut Window) {
    window.resize(PhysicalSize::new(400, 300), 1.0);
    window.poll();
    assert!(window.layout(), "lays out");
}

#[test]
fn click_bubbles_from_the_target() {
    let log = Log::default();
    let (mut window, _) = window(&log);
    open(&mut window);

    window.pointer_event(&down(10.0, 10.0));
    window.pointer_event(&up(10.0, 10.0));
    window.poll();

    assert_eq!(
        *log.borrow(),
        ["inner:click", "outer:click"].map(String::from),
        "clicked inner, then its ancestor"
    );
}

#[test]
fn click_requires_release_over_the_pressed_node() {
    let log = Log::default();
    let (mut window, _) = window(&log);
    open(&mut window);

    window.pointer_event(&down(10.0, 10.0));
    window.pointer_event(&up(10.0, 150.0));

    assert!(
        !log.borrow().iter().any(|entry| entry.ends_with("click")),
        "no click when released elsewhere: {:?}",
        log.borrow()
    );
}

#[test]
fn stop_propagation_skips_ancestors() {
    let log = Log::default();
    let stopper = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let log = log.clone();
        let stopper = stopper.clone();
        move |cx: &UiCx| {
            let parent_log = logger(&log, "parent");
            cx.div()
                .size(size(px(100), px(100)))
                .on_pointer_down(move |_, _| parent_log("down"))
                .children(|cx| {
                    let child_log = logger(&log, "child");
                    let key = cx
                        .div()
                        .size(size(px(50), px(50)))
                        .on_pointer_down(move |cx, _| {
                            child_log("down");
                            cx.stop_propagation();
                        })
                        .build();
                    *stopper.borrow_mut() = Some(key);
                    key
                })
        }
    });
    open(&mut window);

    window.pointer_event(&down(10.0, 10.0));

    assert_eq!(*log.borrow(), ["child:down"].map(String::from));
}

#[test]
fn hover_enters_and_leaves() {
    let log = Log::default();
    let (mut window, _) = window(&log);
    open(&mut window);

    window.pointer_event(&moved(10.0, 10.0));
    window.pointer_event(&moved(20.0, 20.0));
    window.pointer_event(&moved(150.0, 20.0));
    window.pointer_event(&PointerEvent::Leave(MOUSE));

    assert_eq!(
        *log.borrow(),
        ["inner:enter", "inner:leave"].map(String::from),
        "entered once, left when moving to outer"
    );
}

#[test]
fn press_focuses_the_nearest_focusable_node() {
    let log = Log::default();
    let (mut window, nodes) = window(&log);
    open(&mut window);
    let (outer, second) = {
        let nodes = nodes.borrow();
        let nodes = nodes.as_ref().unwrap();
        (nodes.outer, nodes.second)
    };

    window.pointer_event(&down(150.0, 20.0));
    assert_eq!(window.focused(), Some(outer));
    assert_eq!(window.state().focused_node.get_untracked(), Some(outer));

    window.pointer_event(&down(10.0, 150.0));
    assert_eq!(window.focused(), Some(second));
    assert_eq!(
        *log.borrow(),
        ["outer:focus", "outer:blur", "second:focus"].map(String::from)
    );

    // Pressing outside every focusable node clears the focus.
    window.pointer_event(&down(350.0, 250.0));
    assert_eq!(window.focused(), None);
}

#[test]
fn keys_go_to_the_focused_node_and_tab_moves_focus() {
    let log = Log::default();
    let (mut window, nodes) = window(&log);
    open(&mut window);
    let (outer, inner, second) = {
        let nodes = nodes.borrow();
        let nodes = nodes.as_ref().unwrap();
        (nodes.outer, nodes.inner, nodes.second)
    };

    let tab = key_down(Key::Named(NamedKey::Tab), Modifiers::empty());
    let back_tab = key_down(Key::Named(NamedKey::Tab), Modifiers::SHIFT);

    window.keyboard_event(&tab);
    assert_eq!(window.focused(), Some(outer), "tree order starts at outer");
    window.keyboard_event(&tab);
    assert_eq!(window.focused(), Some(inner));
    window.keyboard_event(&tab);
    assert_eq!(window.focused(), Some(second));
    window.keyboard_event(&tab);
    assert_eq!(window.focused(), Some(outer), "wraps around");
    window.keyboard_event(&back_tab);
    assert_eq!(window.focused(), Some(second), "shift goes back");

    log.borrow_mut().clear();
    window.keyboard_event(&tab);
    window.keyboard_event(&tab);
    window.keyboard_event(&key_down(Key::Character("a".into()), Modifiers::empty()));
    assert_eq!(
        log.borrow().last().map(String::as_str),
        Some("outer:key(false)"),
        "a key on inner bubbles to outer"
    );
}

#[test]
fn focus_follows_the_focused_node_signal() {
    let log = Log::default();
    let (mut window, nodes) = window(&log);
    open(&mut window);
    let second = nodes.borrow().as_ref().unwrap().second;

    window.state().focused_node.set(Some(second));
    window.poll();

    assert_eq!(window.focused(), Some(second));
    assert_eq!(*log.borrow(), ["second:focus"].map(String::from));
}

#[test]
fn wheel_scrolls_the_innermost_scroll_container() {
    let mut window = Window::new(|cx: &UiCx| {
        cx.div()
            .size(size(px(100), px(100)))
            .overflow(StylePoint::new(Overflow::Scroll, Overflow::Scroll))
            .children(|cx| cx.div().size(size(px(100), px(300))).build())
    });
    open(&mut window);
    let scroller = window.root();

    window.pointer_event(&PointerEvent::Scroll(PointerScrollEvent {
        pointer: MOUSE,
        delta: ScrollDelta::LineDelta(0.0, -1.0),
        state: state(10.0, 10.0),
    }));

    let top = window
        .ui()
        .tree()
        .borrow()
        .ui
        .prop::<comet_paint::style::ScrollTop>(scroller)
        .map(|top| top.0);
    assert_eq!(top, Some(40.0), "one line down");
    assert!(window.needs_redraw(), "the scroll asks for a frame");
}

#[test]
fn enter_and_space_click_the_focused_node() {
    let log = Log::default();
    let (mut window, nodes) = window(&log);
    open(&mut window);
    let inner = nodes.borrow().as_ref().unwrap().inner;
    let clicks = |log: &Log| {
        log.borrow()
            .iter()
            .filter(|entry| entry.ends_with("click"))
            .cloned()
            .collect::<Vec<_>>()
    };

    // Without focus, nothing is clicked.
    window.keyboard_event(&key_down(Key::Named(NamedKey::Enter), Modifiers::empty()));
    assert!(clicks(&log).is_empty());

    window.state().focused_node.set(Some(inner));
    window.poll();

    window.keyboard_event(&key_down(Key::Named(NamedKey::Enter), Modifiers::empty()));
    assert_eq!(
        clicks(&log),
        ["inner:click", "outer:click"],
        "Enter clicks on press, bubbling"
    );

    log.borrow_mut().clear();
    let space = key_down(Key::Character(" ".into()), Modifiers::empty());
    window.keyboard_event(&space);
    assert!(clicks(&log).is_empty(), "Space does not click on press");
    window.keyboard_event(&KeyboardEvent {
        state: KeyState::Up,
        ..space
    });
    assert_eq!(
        clicks(&log),
        ["inner:click", "outer:click"],
        "Space clicks on release"
    );
}

/// A focusable 100×50 row.
fn row(cx: &UiCx, rows: &RefCell<Vec<NodeKey>>) -> NodeKey {
    let key = cx.div().size(size(px(100), px(50))).focusable(true).build();
    rows.borrow_mut().push(key);
    key
}

/// A 100×100 container scrolling both ways.
fn scroll_box(cx: &UiCx) -> comet_reactivity::NodeBuild<'_> {
    cx.div()
        .size(size(px(100), px(100)))
        .overflow(StylePoint::new(Overflow::Scroll, Overflow::Scroll))
}

fn scroll_top(window: &Window, node: NodeKey) -> f64 {
    window
        .ui()
        .tree()
        .borrow()
        .ui
        .prop::<comet_paint::style::ScrollTop>(node)
        .map_or(0.0, |top| top.0)
}

/// A scroll box holding `count` rows, as the root.
fn list(count: usize) -> (Window, Rc<RefCell<Vec<NodeKey>>>) {
    let rows = Rc::new(RefCell::new(Vec::new()));
    let window = Window::new({
        let rows = rows.clone();
        move |cx: &UiCx| {
            scroll_box(cx).children(|cx| (0..count).map(|_| row(cx, &rows)).collect::<Vec<_>>())
        }
    });

    (window, rows)
}

#[test]
fn focus_scrolls_the_node_into_view() {
    let (mut window, _) = list(6);
    open(&mut window);
    let root = window.root();
    let tab = key_down(Key::Named(NamedKey::Tab), Modifiers::empty());
    let back = key_down(Key::Named(NamedKey::Tab), Modifiers::SHIFT);
    let press = |window: &mut Window, key: &KeyboardEvent| {
        window.keyboard_event(key);
        window.poll();
        window.layout();
        scroll_top(window, root)
    };

    assert_eq!(press(&mut window, &tab), 0.0, "row 0 is visible");
    assert_eq!(press(&mut window, &tab), 0.0, "row 1 is visible");
    assert_eq!(press(&mut window, &tab), 50.0, "row 2 ends at the bottom");
    assert_eq!(press(&mut window, &tab), 100.0, "row 3 ends at the bottom");
    assert_eq!(press(&mut window, &back), 100.0, "row 2 is still visible");
    assert_eq!(press(&mut window, &back), 50.0, "row 1 starts at the top");
    assert_eq!(press(&mut window, &back), 0.0, "row 0 starts at the top");
}

#[test]
fn focus_before_the_first_layout_is_revealed_by_it() {
    let (mut window, rows) = list(5);
    let last = *rows.borrow().last().unwrap();

    window.state().focused_node.set(Some(last));
    window.poll();
    assert_eq!(window.focused(), Some(last));
    open(&mut window);

    assert_eq!(
        scroll_top(&window, window.root()),
        150.0,
        "the last row ends at the bottom"
    );
    assert!(window.needs_redraw(), "the scroll asks for a frame");
}

#[test]
fn focus_scrolls_nested_containers_from_the_innermost() {
    let rows = Rc::new(RefCell::new(Vec::new()));
    let inner = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let (rows, inner) = (rows.clone(), inner.clone());
        move |cx: &UiCx| {
            scroll_box(cx).children(|cx| {
                let spacer = cx.div().size(size(px(100), px(150))).build();
                let key = scroll_box(cx)
                    .children(|cx| (0..4).map(|_| row(cx, &rows)).collect::<Vec<_>>());
                *inner.borrow_mut() = Some(key);
                [spacer, key]
            })
        }
    });
    open(&mut window);
    let inner = inner.borrow().unwrap();
    let last = *rows.borrow().last().unwrap();

    window.state().focused_node.set(Some(last));
    window.poll();

    assert_eq!(
        scroll_top(&window, inner),
        100.0,
        "the row ends at the inner bottom"
    );
    assert_eq!(
        scroll_top(&window, window.root()),
        150.0,
        "the inner box scrolled to show the row"
    );
}
