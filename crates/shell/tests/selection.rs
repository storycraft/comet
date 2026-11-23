//! Text selection by pointer, and focus moving from the selection.

use std::{cell::RefCell, rc::Rc};

use comet_layout::style::unit::*;
use comet_reactivity::{Styled, UiCx};
use comet_shell::{
    Selection, TextPosition, Window,
    style::InputStyled,
    ui_events::{
        keyboard::{Code, Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
        pointer::{
            PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerState,
            PointerType, PointerUpdate,
        },
    },
};
use comet_tree::node::NodeKey;
use dpi::{PhysicalPosition, PhysicalSize};
use reactive_graph::traits::{GetUntracked, Set};

const MOUSE: PointerInfo = PointerInfo {
    pointer_id: Some(PointerId::PRIMARY),
    persistent_device_id: None,
    pointer_type: PointerType::Mouse,
};

const FIRST: &str = "Hello world";
const SECOND: &str = "Second line";

fn state(x: f64, y: f64, count: u8, modifiers: Modifiers) -> PointerState {
    PointerState {
        position: PhysicalPosition::new(x, y),
        count,
        modifiers,
        ..Default::default()
    }
}

fn press(window: &mut Window, x: f64, y: f64, count: u8, modifiers: Modifiers) {
    window.pointer_event(&PointerEvent::Down(PointerButtonEvent {
        button: Some(PointerButton::Primary),
        pointer: MOUSE,
        state: state(x, y, count, modifiers),
    }));
}

fn release(window: &mut Window, x: f64, y: f64) {
    window.pointer_event(&PointerEvent::Up(PointerButtonEvent {
        button: Some(PointerButton::Primary),
        pointer: MOUSE,
        state: state(x, y, 1, Modifiers::empty()),
    }));
}

fn drag(window: &mut Window, x: f64, y: f64) {
    window.pointer_event(&PointerEvent::Move(PointerUpdate {
        pointer: MOUSE,
        current: state(x, y, 1, Modifiers::empty()),
        coalesced: Vec::new(),
        predicted: Vec::new(),
    }));
}

fn tab(window: &mut Window, modifiers: Modifiers) {
    window.keyboard_event(&KeyboardEvent {
        state: KeyState::Down,
        key: Key::Named(NamedKey::Tab),
        code: Code::Tab,
        modifiers,
        ..Default::default()
    });
}

fn open(window: &mut Window) {
    window.resize(PhysicalSize::new(400, 300), 1.0);
    window.poll();
    assert!(window.layout(), "lays out");
}

#[derive(Default)]
struct Nodes {
    before: Option<NodeKey>,
    first: Option<NodeKey>,
    second: Option<NodeKey>,
    after: Option<NodeKey>,
}

/// From the top, 30 high each: a focusable box `before`, paragraphs
/// `first` and `second`, a paragraph `plain` that cannot be selected, and a
/// focusable box `after`. Nothing is below y = 150.
fn window() -> (Window, Rc<RefCell<Nodes>>) {
    let nodes = Rc::new(RefCell::new(Nodes::default()));
    let window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let mut nodes = nodes.borrow_mut();
            let focusable =
                |cx: &UiCx| cx.div().size(size(px(400), px(30))).focusable(true).build();
            let paragraph = |cx: &UiCx, text: &'static str| {
                cx.div()
                    .size(size(px(400), px(30)))
                    .children(|cx| cx.text(text).build())
            };

            let before = focusable(cx);
            let first = paragraph(cx, FIRST);
            let second = paragraph(cx, SECOND);
            let plain = cx
                .div()
                .size(size(px(400), px(30)))
                .selectable(false)
                .children(|cx| cx.text("Not selectable").build());
            let after = focusable(cx);
            *nodes = Nodes {
                before: Some(before),
                first: Some(first),
                second: Some(second),
                after: Some(after),
            };

            cx.div().children(|_| [before, first, second, plain, after])
        }
    });

    (window, nodes)
}

/// The node and index of the anchor and the extent.
fn ends(selection: Selection) -> [(NodeKey, usize); 2] {
    [selection.anchor, selection.extent].map(|position| (position.node, position.index))
}

#[test]
fn press_places_a_caret() {
    let (mut window, nodes) = window();
    open(&mut window);
    let first = nodes.borrow().first.unwrap();

    press(&mut window, 1.0, 40.0, 1, Modifiers::empty());

    let selection = window.selection().expect("a caret");
    assert!(selection.is_collapsed());
    assert_eq!(ends(selection), [(first, 0), (first, 0)]);
    assert_eq!(
        window.state().selection.get_untracked(),
        Some(selection),
        "followed by the signal"
    );
}

#[test]
fn drag_extends_from_the_anchor() {
    let (mut window, nodes) = window();
    open(&mut window);
    let (first, second) = {
        let nodes = nodes.borrow();
        (nodes.first.unwrap(), nodes.second.unwrap())
    };

    press(&mut window, 1.0, 40.0, 1, Modifiers::empty());
    drag(&mut window, 390.0, 40.0);
    assert_eq!(
        ends(window.selection().unwrap()),
        [(first, 0), (first, FIRST.len())],
        "to the end of the line"
    );

    drag(&mut window, 390.0, 70.0);
    assert_eq!(
        ends(window.selection().unwrap()),
        [(first, 0), (second, SECOND.len())],
        "into the next paragraph"
    );

    // Over no text, the extent stays in the text it is in.
    drag(&mut window, 1.0, 250.0);
    let [_, (node, _)] = ends(window.selection().unwrap());
    assert_eq!(node, second);

    release(&mut window, 1.0, 250.0);
    let selection = window.selection();
    drag(&mut window, 1.0, 40.0);
    assert_eq!(
        window.selection(),
        selection,
        "moves after release do not select"
    );
}

#[test]
fn repeated_presses_select_a_word_then_a_line() {
    let (mut window, nodes) = window();
    open(&mut window);
    let first = nodes.borrow().first.unwrap();

    press(&mut window, 1.0, 40.0, 2, Modifiers::empty());
    assert_eq!(
        ends(window.selection().unwrap()),
        [(first, 0), (first, "Hello".len())],
        "a word"
    );

    press(&mut window, 1.0, 40.0, 3, Modifiers::empty());
    let [(_, start), (_, end)] = ends(window.selection().unwrap());
    assert_eq!((start, end.min(FIRST.len())), (0, FIRST.len()), "the line");
}

#[test]
fn shift_press_extends_the_selection() {
    let (mut window, nodes) = window();
    open(&mut window);
    let (first, second) = {
        let nodes = nodes.borrow();
        (nodes.first.unwrap(), nodes.second.unwrap())
    };

    press(&mut window, 1.0, 40.0, 1, Modifiers::empty());
    release(&mut window, 1.0, 40.0);
    press(&mut window, 390.0, 70.0, 1, Modifiers::SHIFT);

    assert_eq!(
        ends(window.selection().unwrap()),
        [(first, 0), (second, SECOND.len())]
    );
}

#[test]
fn presses_outside_selectable_text_clear_the_selection() {
    let (mut window, _) = window();
    open(&mut window);

    press(&mut window, 1.0, 40.0, 1, Modifiers::empty());
    assert!(window.selection().is_some());
    press(&mut window, 1.0, 100.0, 1, Modifiers::empty());
    assert_eq!(window.selection(), None, "the text cannot be selected");

    press(&mut window, 1.0, 40.0, 1, Modifiers::empty());
    press(&mut window, 1.0, 250.0, 1, Modifiers::empty());
    assert_eq!(window.selection(), None, "no text");
}

#[test]
fn tab_without_focus_starts_from_the_selection() {
    let (mut window, nodes) = window();
    open(&mut window);
    let (before, after) = {
        let nodes = nodes.borrow();
        (nodes.before.unwrap(), nodes.after.unwrap())
    };

    // Pressing text clears the focus.
    press(&mut window, 1.0, 70.0, 1, Modifiers::empty());
    assert_eq!(window.focused(), None);
    tab(&mut window, Modifiers::empty());
    assert_eq!(window.focused(), Some(after), "the next focusable node");

    window.state().focused_node.set(None);
    window.poll();
    tab(&mut window, Modifiers::SHIFT);
    assert_eq!(
        window.focused(),
        Some(before),
        "the previous focusable node"
    );
}

#[test]
fn selection_follows_the_signal_and_the_tree() {
    let (mut window, nodes) = window();
    open(&mut window);
    let (first, second) = {
        let nodes = nodes.borrow();
        (nodes.first.unwrap(), nodes.second.unwrap())
    };
    let selection = Selection::new(
        TextPosition::new(first, 2, Default::default()),
        TextPosition::new(second, 3, Default::default()),
    );

    window.state().selection.set(Some(selection));
    window.poll();
    assert_eq!(window.selection(), Some(selection));

    window.ui().tree().borrow_mut().ui.remove_parent(second);
    window.poll();
    assert_eq!(window.selection(), None, "its text left the tree");
    assert_eq!(window.state().selection.get_untracked(), None);
}

#[test]
fn tab_without_focus_starts_from_the_extent() {
    // From the top, 30 high each: focusable, paragraph, focusable,
    // paragraph, focusable.
    let nodes = Rc::new(RefCell::new(Vec::new()));
    let mut window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let focusable =
                |cx: &UiCx| cx.div().size(size(px(400), px(30))).focusable(true).build();
            let paragraph = |cx: &UiCx, text: &'static str| {
                cx.div()
                    .size(size(px(400), px(30)))
                    .children(|cx| cx.text(text).build())
            };
            let children = [
                focusable(cx),
                paragraph(cx, FIRST),
                focusable(cx),
                paragraph(cx, SECOND),
                focusable(cx),
            ];
            *nodes.borrow_mut() = children.to_vec();

            cx.div().children(|_| children)
        }
    });
    open(&mut window);
    let [top, first, middle, second, _] = nodes.borrow()[..] else {
        unreachable!()
    };

    // Selected backwards, from the second paragraph to the first.
    press(&mut window, 390.0, 100.0, 1, Modifiers::empty());
    drag(&mut window, 1.0, 40.0);
    release(&mut window, 1.0, 40.0);
    let selection = window.selection().unwrap();
    assert_eq!(
        (selection.anchor.node, selection.extent.node),
        (second, first)
    );

    tab(&mut window, Modifiers::empty());
    assert_eq!(
        window.focused(),
        Some(middle),
        "after the extent, not the anchor"
    );

    window.state().focused_node.set(None);
    window.poll();
    tab(&mut window, Modifiers::SHIFT);
    assert_eq!(window.focused(), Some(top), "before the extent");
}
