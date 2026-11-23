//! Text selection by pointer, and focus moving from the selection.

use std::{cell::RefCell, rc::Rc};

use comet_layout::style::{geometry::AutoValue, unit::*};
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
                .selectable(AutoValue::Value(false))
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
fn tab_without_focus_moves_around_the_selection() {
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
    let [top, first, middle, second, bottom] = nodes.borrow()[..] else {
        unreachable!()
    };
    assert_ne!(middle, top);

    // Forwards, from the first paragraph to the second, then backwards.
    for (from, to, ends) in [
        ((1.0, 40.0), (390.0, 100.0), (first, second)),
        ((390.0, 100.0), (1.0, 40.0), (second, first)),
    ] {
        window.state().focused_node.set(None);
        window.poll();
        press(&mut window, from.0, from.1, 1, Modifiers::empty());
        drag(&mut window, to.0, to.1);
        release(&mut window, to.0, to.1);
        let selection = window.selection().unwrap();
        assert_eq!((selection.anchor.node, selection.extent.node), ends);

        tab(&mut window, Modifiers::empty());
        assert_eq!(
            window.focused(),
            Some(bottom),
            "after the selection, not inside it"
        );

        window.state().focused_node.set(None);
        window.poll();
        tab(&mut window, Modifiers::SHIFT);
        assert_eq!(window.focused(), Some(top), "before the selection");
    }
}

#[test]
fn unselectable_spans_in_selectable_text_are_not_selected() {
    let paragraph = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let paragraph = paragraph.clone();
        move |cx: &UiCx| {
            let key = cx.div().size(size(px(400), px(30))).children(|cx| {
                [
                    cx.text("Selectable ").build(),
                    cx.div()
                        .display_outer(comet_layout::style::bx::DisplayOuter::Inline)
                        .selectable(AutoValue::Value(false))
                        .children(|cx| cx.text("not selectable").build()),
                ]
            });
            *paragraph.borrow_mut() = Some(key);
            cx.div().children(|_| [key])
        }
    });
    open(&mut window);
    let paragraph = paragraph.borrow().unwrap();

    press(&mut window, 1.0, 10.0, 1, Modifiers::empty());
    let selection = window.selection().expect("the paragraph's own text");
    assert_eq!(selection.anchor.node, paragraph, "kept in the paragraph");

    // Far into the span, past "Selectable ".
    press(&mut window, 150.0, 10.0, 1, Modifiers::empty());
    assert_eq!(window.selection(), None, "the span cannot be selected");
}

#[test]
fn auto_keeps_whether_the_node_above_is_selectable() {
    // An unselectable paragraph holding a selectable span, then a span
    // keeping the paragraph's choice.
    let paragraph = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let paragraph = paragraph.clone();
        move |cx: &UiCx| {
            let span = |cx: &UiCx, text: &'static str, selectable| {
                cx.div()
                    .display_outer(comet_layout::style::bx::DisplayOuter::Inline)
                    .selectable(selectable)
                    .children(|cx| cx.text(text).build())
            };
            let key = cx
                .div()
                .size(size(px(400), px(30)))
                .selectable(AutoValue::Value(false))
                .children(|cx| {
                    [
                        span(cx, "Selectable", AutoValue::Value(true)),
                        span(cx, " not selectable", AutoValue::Auto),
                    ]
                });
            *paragraph.borrow_mut() = Some(key);
            cx.div().children(|_| [key])
        }
    });
    open(&mut window);
    let paragraph = paragraph.borrow().unwrap();

    press(&mut window, 1.0, 10.0, 1, Modifiers::empty());
    let selection = window.selection().expect("the selectable span");
    assert_eq!(selection.anchor.node, paragraph, "kept in the paragraph");

    press(&mut window, 150.0, 10.0, 1, Modifiers::empty());
    assert_eq!(
        window.selection(),
        None,
        "the span keeps the paragraph's choice"
    );
}

#[test]
fn text_between_block_boxes_is_selected_in_its_run() {
    // A paragraph whose text is split by a focusable block box into two
    // runs, then a focusable box.
    let nodes = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let focusable =
                |cx: &UiCx| cx.div().size(size(px(400), px(40))).focusable(true).build();
            let block = focusable(cx);
            let paragraph = cx
                .div()
                .children(|cx| [cx.text(FIRST).build(), block, cx.text(SECOND).build()]);
            let after = focusable(cx);
            *nodes.borrow_mut() = Some((paragraph, block, after));
            cx.div().children(|_| [paragraph, after])
        }
    });
    open(&mut window);
    let (paragraph, block, after) = nodes.borrow().unwrap();
    // The second run starts below the block box.
    let second_y = {
        let trees = &window.pipeline().trees;
        let layout = trees.fragments.items[trees.box_fragment(block).unwrap()].layout;
        layout.location.y + layout.size.height + 5.0
    };

    press(&mut window, 1.0, 5.0, 1, Modifiers::empty());
    let selection = window.selection().expect("the first run");
    assert_eq!(selection.anchor.node, paragraph, "kept in the paragraph");
    assert_eq!(selection.anchor.run, None, "in the first run");

    drag(&mut window, 399.0, second_y);
    release(&mut window, 399.0, second_y);
    let selection = window.selection().unwrap();
    assert_eq!(selection.anchor.run, None, "anchored in the first run");
    assert_eq!(
        (
            selection.extent.node,
            selection.extent.run,
            selection.extent.index
        ),
        (paragraph, Some(block), SECOND.len()),
        "extended to the end of the second run"
    );
    assert!(!selection.is_collapsed());

    press(&mut window, 2.0, second_y, 2, Modifiers::empty());
    let selection = window.selection().unwrap();
    assert_eq!(
        [selection.anchor, selection.extent].map(|end| (end.run, end.index)),
        [(Some(block), 0), (Some(block), "Second".len())],
        "a word of the second run"
    );

    // Focus moves on from after the block box the run follows.
    tab(&mut window, Modifiers::empty());
    assert_eq!(window.focused(), Some(after), "past the block box");
    window.state().focused_node.set(None);
    window.poll();
    tab(&mut window, Modifiers::SHIFT);
    assert_eq!(
        window.focused(),
        Some(block),
        "the block box before the run"
    );

    // The run goes with the block box it follows.
    window.ui().tree().borrow_mut().ui.remove_parent(block);
    window.poll();
    assert_eq!(window.selection(), None, "its run left the tree");
}

#[test]
fn the_highlight_covers_the_selected_text_in_tree_order() {
    let (mut window, nodes) = window();
    open(&mut window);
    let (first, second) = {
        let nodes = nodes.borrow();
        (nodes.first.unwrap(), nodes.second.unwrap())
    };
    let set = |window: &mut Window, anchor, extent| {
        window
            .state()
            .selection
            .set(Some(Selection::new(anchor, extent)));
        window.poll();
        window.selection_rects()
    };
    let at = |node, index| TextPosition::new(node, index, Default::default());

    assert!(
        set(&mut window, at(first, 3), at(first, 3)).is_empty(),
        "a caret"
    );

    // "world" of the first paragraph, made either way.
    let world = set(&mut window, at(first, 6), at(first, FIRST.len()));
    assert_eq!(
        world,
        set(&mut window, at(first, FIRST.len()), at(first, 6))
    );
    let [rect] = world.as_slice() else {
        panic!("one line: {world:?}");
    };
    assert!(rect.x0 > 1.0 && rect.width() > 0.0, "after \"Hello \"");
    assert!(rect.y0 >= 30.0 && rect.y1 <= 60.0, "in the first paragraph");

    // From "world" to "Second", backwards: both paragraphs, in order.
    let rects = set(&mut window, at(second, "Second".len()), at(first, 6));
    let [start, end] = rects.as_slice() else {
        panic!("one line each: {rects:?}");
    };
    assert_eq!(start.x0, rect.x0, "from \"world\"");
    assert!(start.y1 <= 60.0 && end.y0 >= 60.0 && end.y1 <= 90.0);
    assert!(end.x0 < 1.0 && end.x1 < rect.x1, "to the end of \"Second\"");
}

#[test]
fn the_highlight_covers_the_runs_between_block_boxes() {
    let nodes = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let block = cx.div().size(size(px(400), px(40))).build();
            let paragraph = cx
                .div()
                .children(|cx| [cx.text(FIRST).build(), block, cx.text(SECOND).build()]);
            *nodes.borrow_mut() = Some((paragraph, block));
            cx.div().children(|_| [paragraph])
        }
    });
    open(&mut window);
    let (paragraph, block) = nodes.borrow().unwrap();

    window.state().selection.set(Some(Selection::new(
        TextPosition::new(paragraph, 6, Default::default()),
        TextPosition::new(paragraph, 3, Default::default()).in_run(Some(block)),
    )));
    window.poll();
    let rects = window.selection_rects();
    let [first, second] = rects.as_slice() else {
        panic!("one rectangle per run: {rects:?}");
    };
    assert!(
        first.y1 <= second.y0 - 40.0,
        "the block box is between them"
    );
}

#[test]
fn the_highlight_goes_into_and_out_of_inline_boxes_in_order() {
    use comet_layout::style::bx::{DisplayInner, DisplayOuter};

    // "Hello [inner] world", the brackets an inline box with its own text.
    let nodes = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let inline_box = cx
                .div()
                .display_outer(DisplayOuter::Inline)
                .display_inner(DisplayInner::FlowRoot)
                .children(|cx| cx.text("inner").build());
            let paragraph = cx.div().size(size(px(400), px(30))).children(|cx| {
                [
                    cx.text("Hello ").build(),
                    inline_box,
                    cx.text(" world").build(),
                ]
            });
            *nodes.borrow_mut() = Some((paragraph, inline_box));
            cx.div().children(|_| [paragraph])
        }
    });
    open(&mut window);
    let (paragraph, inline_box) = nodes.borrow().unwrap();
    let bounds = {
        let trees = &window.pipeline().trees;
        let fragment = trees.box_fragment(inline_box).unwrap();
        let layout = trees.fragments.items[fragment].layout;
        trees
            .fragment_to_scene(fragment)
            .unwrap()
            .transform_rect_bbox(layout.border_box())
    };
    let rects = |window: &mut Window, anchor, extent| {
        window
            .state()
            .selection
            .set(Some(Selection::new(anchor, extent)));
        window.poll();
        let rects = window.selection_rects();
        let before = rects
            .iter()
            .filter(|rect| rect.x1 <= bounds.x0 + 0.5)
            .count();
        let inside = rects
            .iter()
            .filter(|rect| rect.x0 >= bounds.x0 - 0.5 && rect.x1 <= bounds.x1 + 0.5)
            .count();
        let after = rects
            .iter()
            .filter(|rect| rect.x0 >= bounds.x1 - 0.5)
            .count();
        assert_eq!(
            before + inside + after,
            rects.len(),
            "nothing over the box edges: {rects:?}"
        );
        (before, inside, after)
    };
    let outer = |index| TextPosition::new(paragraph, index, Default::default());
    let inner = |index| TextPosition::new(inline_box, index, Default::default());
    // "Hello " is 6 bytes, then the box's U+FFFC of 3.
    let box_end = 6 + '\u{FFFC}'.len_utf8();

    assert_eq!(
        rects(&mut window, outer(1), inner(3)),
        (1, 1, 0),
        "into the box"
    );
    assert_eq!(
        rects(&mut window, inner(3), outer(1)),
        (1, 1, 0),
        "either way"
    );
    assert_eq!(
        rects(&mut window, inner(2), outer(box_end + 3)),
        (0, 1, 1),
        "out of the box"
    );
    assert_eq!(
        rects(&mut window, outer(1), outer(box_end + 3)),
        (1, 1, 1),
        "across the whole box: its text, not its character"
    );
}
