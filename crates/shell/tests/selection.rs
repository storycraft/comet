//! Text selection by pointer, and focus moving from the selection.

use std::{cell::RefCell, rc::Rc};

use comet_layout::style::{geometry::AutoValue, unit::*};
use comet_paint::selection::{Selection, TextPosition};
use comet_reactivity::{cx::UiCx, style::Styled};
use comet_shell::{style::InputStyled, window::Window};
use comet_tree::node::NodeKey;
use dpi::{PhysicalPosition, PhysicalSize};
use reactive_graph::traits::{Get, GetUntracked, Set, Update};
use ui_events::{
    keyboard::{Code, Key, KeyState, KeyboardEvent, Modifiers, NamedKey},
    pointer::{
        PointerButton, PointerButtonEvent, PointerEvent, PointerId, PointerInfo, PointerState,
        PointerType, PointerUpdate,
    },
};

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

/// From the top, 30 high each: a focusable box `before`, paragraphs of the
/// text nodes `first` and `second`, a paragraph `plain` that cannot be
/// selected, and a focusable box `after`. Nothing is below y = 150.
fn window() -> (Window, Rc<RefCell<Nodes>>) {
    let nodes = Rc::new(RefCell::new(Nodes::default()));
    let window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let mut nodes = nodes.borrow_mut();
            let focusable =
                |cx: &UiCx| cx.div().size(size(px(400), px(30))).focusable(true).build();
            let paragraph =
                |cx: &UiCx, text: NodeKey| cx.div().size(size(px(400), px(30))).children(|_| text);

            let before = focusable(cx);
            let first = cx.text(FIRST).build();
            let second = cx.text(SECOND).build();
            let (first_paragraph, second_paragraph) = (paragraph(cx, first), paragraph(cx, second));
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

            cx.div()
                .children(|_| [before, first_paragraph, second_paragraph, plain, after])
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
            let paragraph =
                |cx: &UiCx, text: NodeKey| cx.div().size(size(px(400), px(30))).children(|_| text);
            let (first, second) = (cx.text(FIRST).build(), cx.text(SECOND).build());
            let (top, middle, bottom) = (focusable(cx), focusable(cx), focusable(cx));
            *nodes.borrow_mut() = vec![top, first, middle, second, bottom];

            cx.div().children(|cx| {
                [
                    top,
                    paragraph(cx, first),
                    middle,
                    paragraph(cx, second),
                    bottom,
                ]
            })
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
    let text = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let text = text.clone();
        move |cx: &UiCx| {
            let selectable = cx.text("Selectable ").build();
            *text.borrow_mut() = Some(selectable);
            let key = cx.div().size(size(px(400), px(30))).children(|cx| {
                [
                    selectable,
                    cx.div()
                        .display_outer(comet_layout::style::bx::DisplayOuter::Inline)
                        .selectable(AutoValue::Value(false))
                        .children(|cx| cx.text("not selectable").build()),
                ]
            });
            cx.div().children(|_| [key])
        }
    });
    open(&mut window);
    let text = text.borrow().unwrap();

    press(&mut window, 1.0, 10.0, 1, Modifiers::empty());
    let selection = window.selection().expect("the paragraph's own text");
    assert_eq!(selection.anchor.node, text, "kept in its text node");

    // Far into the span, past "Selectable ".
    press(&mut window, 150.0, 10.0, 1, Modifiers::empty());
    assert_eq!(window.selection(), None, "the span cannot be selected");
}

#[test]
fn auto_keeps_whether_the_node_above_is_selectable() {
    // An unselectable paragraph holding a selectable span, then a span
    // keeping the paragraph's choice.
    let text = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let text = text.clone();
        move |cx: &UiCx| {
            let span = |cx: &UiCx, value: &'static str, selectable| {
                let node = cx.text(value).build();
                text.borrow_mut().get_or_insert(node);
                cx.div()
                    .display_outer(comet_layout::style::bx::DisplayOuter::Inline)
                    .selectable(selectable)
                    .children(|_| node)
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
            cx.div().children(|_| [key])
        }
    });
    open(&mut window);
    let text = text.borrow().unwrap();

    press(&mut window, 1.0, 10.0, 1, Modifiers::empty());
    let selection = window.selection().expect("the selectable span");
    assert_eq!(selection.anchor.node, text, "in the span's text node");

    press(&mut window, 150.0, 10.0, 1, Modifiers::empty());
    assert_eq!(
        window.selection(),
        None,
        "the span keeps the paragraph's choice"
    );
}

#[test]
fn text_between_block_boxes_is_selected() {
    // A paragraph whose text is split by a focusable block box, so each
    // text is laid out in its own anonymous block, then a focusable box.
    let nodes = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let focusable =
                |cx: &UiCx| cx.div().size(size(px(400), px(40))).focusable(true).build();
            let block = focusable(cx);
            let (first, second) = (cx.text(FIRST).build(), cx.text(SECOND).build());
            let paragraph = cx.div().children(|_| [first, block, second]);
            let after = focusable(cx);
            *nodes.borrow_mut() = Some((first, block, second, after));
            cx.div().children(|_| [paragraph, after])
        }
    });
    open(&mut window);
    let (first, block, second, after) = nodes.borrow().unwrap();
    // The second text starts below the block box.
    let second_y = {
        let trees = &window.pipeline().trees;
        let layout = &trees.fragments.boxes[trees.box_fragment(block).unwrap()]
            .layout
            .geometry;
        layout.location.y + layout.size.height + 5.0
    };

    press(&mut window, 1.0, 5.0, 1, Modifiers::empty());
    let selection = window.selection().expect("the first text");
    assert_eq!(ends(selection), [(first, 0), (first, 0)]);

    drag(&mut window, 399.0, second_y);
    release(&mut window, 399.0, second_y);
    assert_eq!(
        ends(window.selection().unwrap()),
        [(first, 0), (second, SECOND.len())],
        "extended to the end of the second text"
    );

    press(&mut window, 2.0, second_y, 2, Modifiers::empty());
    assert_eq!(
        ends(window.selection().unwrap()),
        [(second, 0), (second, "Second".len())],
        "a word of the second text"
    );

    // Focus moves on from the text, after the block box.
    tab(&mut window, Modifiers::empty());
    assert_eq!(window.focused(), Some(after), "past the block box");
    window.state().focused_node.set(None);
    window.poll();
    tab(&mut window, Modifiers::SHIFT);
    assert_eq!(window.focused(), Some(block), "the block box before");

    // Positions are kept in text nodes, whatever inline layout holds them.
    let selection = window.selection();
    window.ui().tree().borrow_mut().ui.remove_parent(block);
    window.poll();
    assert!(window.layout(), "lays out the merged text");
    assert_eq!(window.selection(), selection, "kept as the runs merge");
    assert_eq!(window.selection_rects().len(), 1, "and highlighted");
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
            let (first, second) = (cx.text(FIRST).build(), cx.text(SECOND).build());
            let paragraph = cx.div().children(|_| [first, block, second]);
            *nodes.borrow_mut() = Some((first, second));
            cx.div().children(|_| [paragraph])
        }
    });
    open(&mut window);
    let (first_text, second_text) = nodes.borrow().unwrap();

    window.state().selection.set(Some(Selection::new(
        TextPosition::new(first_text, 6, Default::default()),
        TextPosition::new(second_text, 3, Default::default()),
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
            let (hello, inner, world) = (
                cx.text("Hello ").build(),
                cx.text("inner").build(),
                cx.text(" world").build(),
            );
            let inline_box = cx
                .div()
                .display_outer(DisplayOuter::Inline)
                .display_inner(DisplayInner::FlowRoot)
                .children(|_| inner);
            let paragraph = cx
                .div()
                .size(size(px(400), px(30)))
                .children(|_| [hello, inline_box, world]);
            *nodes.borrow_mut() = Some((hello, inline_box, inner, world));
            cx.div().children(|_| [paragraph])
        }
    });
    open(&mut window);
    let (hello, inline_box, inner, world) = nodes.borrow().unwrap();
    let bounds = {
        let trees = &window.pipeline().trees;
        let fragment = trees.box_fragment(inline_box).unwrap();
        let layout = &trees.fragments.boxes[fragment].layout.geometry;
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
    let at = |node, index| TextPosition::new(node, index, Default::default());
    let (hello, world) = (|index| at(hello, index), |index| at(world, index));
    let inner = |index| at(inner, index);

    assert_eq!(
        rects(&mut window, hello(1), inner(3)),
        (1, 1, 0),
        "into the box"
    );
    assert_eq!(
        rects(&mut window, inner(3), hello(1)),
        (1, 1, 0),
        "either way"
    );
    assert_eq!(
        rects(&mut window, inner(2), world(3)),
        (0, 1, 1),
        "out of the box"
    );
    assert_eq!(
        rects(&mut window, hello(1), world(3)),
        (1, 1, 1),
        "across the whole box: its text, not its character"
    );
}

#[test]
fn positions_are_offsets_into_their_own_text_node() {
    // "Hello " and "world" in one paragraph, "world" in a span.
    let nodes = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let (hello, world) = (cx.text("Hello ").build(), cx.text("world").build());
            *nodes.borrow_mut() = Some((hello, world));
            let span = cx
                .div()
                .display_outer(comet_layout::style::bx::DisplayOuter::Inline)
                .children(|_| world);
            let paragraph = cx
                .div()
                .size(size(px(400), px(30)))
                .children(|_| [hello, span]);
            cx.div().children(|_| [paragraph])
        }
    });
    open(&mut window);
    let (hello, world) = nodes.borrow().unwrap();

    press(&mut window, 1.0, 10.0, 1, Modifiers::empty());
    drag(&mut window, 399.0, 10.0);
    assert_eq!(
        ends(window.selection().unwrap()),
        [(hello, 0), (world, "world".len())],
        "from the first text node to the end of the second"
    );

    press(&mut window, 399.0, 10.0, 2, Modifiers::empty());
    assert_eq!(
        ends(window.selection().unwrap()),
        [(world, 0), (world, "world".len())],
        "\"world\" from the start of its own node"
    );
}

#[test]
fn presses_outside_of_text_select_in_the_nearest_text() {
    let (mut window, nodes) = window();
    open(&mut window);
    let (first, second) = {
        let nodes = nodes.borrow();
        (nodes.first.unwrap(), nodes.second.unwrap())
    };

    // On the box above the paragraphs: the first, nearest to the pointer.
    press(&mut window, 1.0, 10.0, 1, Modifiers::empty());
    assert_eq!(ends(window.selection().unwrap()), [(first, 0), (first, 0)]);
    press(&mut window, 390.0, 10.0, 1, Modifiers::empty());
    assert_eq!(
        ends(window.selection().unwrap()),
        [(first, FIRST.len()), (first, FIRST.len())],
        "at the end, closest to the pointer"
    );

    // On the box below: the second paragraph, past the one that cannot be
    // selected.
    press(&mut window, 390.0, 135.0, 1, Modifiers::empty());
    assert_eq!(
        ends(window.selection().unwrap()),
        [(second, SECOND.len()), (second, SECOND.len())]
    );

    // Dragging from there over the box above extends to the nearest text.
    drag(&mut window, 1.0, 10.0);
    assert_eq!(
        ends(window.selection().unwrap()),
        [(second, SECOND.len()), (first, 0)]
    );
    release(&mut window, 1.0, 10.0);
    assert_eq!(window.selection_rects().len(), 2, "both paragraphs");

    // Shift extends to the nearest text too.
    press(&mut window, 1.0, 40.0, 1, Modifiers::empty());
    press(&mut window, 390.0, 135.0, 1, Modifiers::SHIFT);
    assert_eq!(
        ends(window.selection().unwrap()),
        [(first, 0), (second, SECOND.len())]
    );
}

/// The chunks of the text `anchor` to `extent` select in `window`.
fn chunks(window: &Window, anchor: TextPosition, extent: TextPosition) -> Vec<String> {
    window.with_selected_text(Selection::new(anchor, extent), |text| {
        text.map(str::to_owned).collect()
    })
}

#[test]
fn selected_text_is_extracted_in_tree_order() {
    let (mut window, nodes) = window();
    open(&mut window);
    let (first, second) = {
        let nodes = nodes.borrow();
        (nodes.first.unwrap(), nodes.second.unwrap())
    };
    let at = |node, index| TextPosition::new(node, index, Default::default());

    assert!(
        chunks(&window, at(first, 3), at(first, 3)).is_empty(),
        "a caret"
    );
    assert_eq!(chunks(&window, at(first, 0), at(first, 5)), ["Hello"]);
    assert_eq!(
        chunks(&window, at(first, 6), at(second, 6)),
        ["world", "\n", "Second"],
        "a line break between paragraphs"
    );
    assert_eq!(
        chunks(&window, at(second, 6), at(first, 6)),
        ["world", "\n", "Second"],
        "either way"
    );

    // The current selection, made with the pointer.
    press(&mut window, 1.0, 40.0, 1, Modifiers::empty());
    drag(&mut window, 390.0, 70.0);
    let text = window.selected_text(window.selection().unwrap());
    assert_eq!(text, format!("{FIRST}\n{SECOND}"));
}

#[test]
fn selected_text_is_chunked_by_text_node_without_unselectable_text() {
    use comet_layout::style::bx::{DisplayInner, DisplayOuter};

    // "One [two ]three [inner] four", "two " unselectable, "inner" in an
    // inline box.
    let nodes = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let (one, three, four) = (
                cx.text("One ").build(),
                cx.text("three ").build(),
                cx.text(" four").build(),
            );
            let span = cx
                .div()
                .display_outer(DisplayOuter::Inline)
                .selectable(AutoValue::Value(false))
                .children(|cx| cx.text("two ").build());
            let inline_box = cx
                .div()
                .display_outer(DisplayOuter::Inline)
                .display_inner(DisplayInner::FlowRoot)
                .children(|cx| cx.text("inner").build());
            *nodes.borrow_mut() = Some((one, four));
            let paragraph = cx
                .div()
                .size(size(px(400), px(30)))
                .children(|_| [one, span, three, inline_box, four]);
            cx.div().children(|_| [paragraph])
        }
    });
    open(&mut window);
    let (one, four) = nodes.borrow().unwrap();
    let at = |node, index| TextPosition::new(node, index, Default::default());

    assert_eq!(
        chunks(&window, at(one, 0), at(four, " four".len())),
        ["One ", "three ", "inner", " four"],
        "no break around the inline box"
    );
}

#[test]
fn selected_text_breaks_between_runs_of_block_boxes() {
    let nodes = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let block = cx.div().size(size(px(400), px(40))).build();
            let (first, second) = (cx.text(FIRST).build(), cx.text(SECOND).build());
            *nodes.borrow_mut() = Some((first, second));
            let paragraph = cx.div().children(|_| [first, block, second]);
            cx.div().children(|_| [paragraph])
        }
    });
    open(&mut window);
    let (first, second) = nodes.borrow().unwrap();
    let text = window.selected_text(Selection::new(
        TextPosition::new(first, 0, Default::default()),
        TextPosition::new(second, SECOND.len(), Default::default()),
    ));

    assert_eq!(text, format!("{FIRST}\n{SECOND}"));
}

#[test]
fn selection_follows_the_scroll_of_the_box_holding_the_text() {
    use comet_layout::style::geometry::Point as StylePoint;
    use comet_paint::style::ScrollTop;
    use taffy::Overflow;

    const TEXT: &str = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
    // A paragraph scrolling its wrapped text.
    let nodes = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let text = cx.text(TEXT).build();
            let paragraph = cx
                .div()
                .size(size(px(100), px(40)))
                .overflow(StylePoint::new(Overflow::Visible, Overflow::Scroll))
                .children(|_| text);
            *nodes.borrow_mut() = Some((paragraph, text));
            cx.div().children(|_| [paragraph])
        }
    });
    open(&mut window);
    let (paragraph, text) = nodes.borrow().unwrap();
    let theta = TEXT.find("theta").unwrap();
    let at = |index| TextPosition::new(text, index, Default::default());
    let theta_rect = |window: &mut Window| {
        window
            .state()
            .selection
            .set(Some(Selection::new(at(theta), at(theta + "theta".len()))));
        window.poll();
        let rects = window.selection_rects();
        let [rect] = rects.as_slice() else {
            panic!("one line: {rects:?}");
        };
        *rect
    };

    let unscrolled = theta_rect(&mut window);
    assert!(unscrolled.y1 > 40.0, "below the paragraph: {unscrolled:?}");
    let range = window.pipeline().trees.scroll_range(paragraph).unwrap();
    let scroll = unscrolled.y0.min(range.y);
    window
        .ui()
        .tree()
        .borrow_mut()
        .set_prop(paragraph, ScrollTop(scroll));
    window.poll();
    assert!(window.layout(), "lays out");

    let scrolled = theta_rect(&mut window);
    assert!(
        (scrolled.y0 - (unscrolled.y0 - scroll)).abs() < 0.01,
        "the highlight moves with the text: {scrolled:?}"
    );
    assert!(scrolled.y1 <= 40.0, "in view: {scrolled:?}");

    // A double press on the word, where it is drawn, selects it.
    let center = scrolled.center();
    press(&mut window, center.x, center.y, 2, Modifiers::empty());
    assert_eq!(
        ends(window.selection().unwrap()),
        [(text, theta), (text, theta + "theta".len())]
    );
}

#[test]
fn selected_text_reads_boxes_out_of_flow_once_after_the_text() {
    use taffy::Position;

    // "Hello [inside] world" with "inside" in a positioned box taken out
    // of the flow, then a paragraph "after".
    let nodes = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let nodes = nodes.clone();
        move |cx: &UiCx| {
            let (hello, world, after) = (
                cx.text("Hello ").build(),
                cx.text("world").build(),
                cx.text("after").build(),
            );
            let positioned = cx
                .div()
                .position(Position::Absolute)
                .children(|cx| cx.text("inside").build());
            let paragraph = cx
                .div()
                .size(size(px(400), px(30)))
                .children(|_| [hello, positioned, world]);
            let next = cx.div().size(size(px(400), px(30))).children(|_| after);
            *nodes.borrow_mut() = Some((hello, after));
            cx.div().children(|_| [paragraph, next])
        }
    });
    open(&mut window);
    let (hello, after) = nodes.borrow().unwrap();
    let at = |node, index| TextPosition::new(node, index, Default::default());

    assert_eq!(
        chunks(&window, at(hello, 0), at(after, "after".len())),
        ["Hello ", "world", "\n", "inside", "\n", "after"]
    );
}

#[test]
fn text_wrapped_in_a_container_after_an_unselectable_box_is_selectable() {
    use comet_reactivity::style::FlexStyled;

    // A label after an unselectable button in a flex row, like the counter
    // of the examples: the label is wrapped in a box of its own.
    let text = Rc::new(RefCell::new(None));
    let mut window = Window::new({
        let text = text.clone();
        move |cx: &UiCx| {
            let (count, set_count) = reactive_graph::signal::signal(0u32);
            let label = cx
                .text_with(move || format!("Clicked {} times", count.get()))
                .build();
            *text.borrow_mut() = Some(label);
            let button = cx
                .div()
                .size(size(px(100), px(30)))
                .selectable(AutoValue::Value(false))
                .on_click(move |_, _| set_count.update(|count| *count += 1))
                .children(|cx| cx.text("Count up").build());
            let row = cx.div().flex().children(|_| [button, label]);
            cx.div().children(|_| [row])
        }
    });
    open(&mut window);
    let text = text.borrow().unwrap();
    let whole = |len| {
        Selection::new(
            TextPosition::new(text, 0, Default::default()),
            TextPosition::new(text, len, Default::default()),
        )
    };

    // Pressing and dragging over the label selects it, and highlights it.
    press(&mut window, 110.0, 10.0, 1, Modifiers::empty());
    drag(&mut window, 390.0, 10.0);
    let selection = window.selection().expect("the label is selectable");
    assert_eq!(selection.anchor.node, text);
    assert_eq!(selection.extent.node, text);
    assert_eq!(window.selection_rects().len(), 1, "highlighted");
    assert_eq!(window.selected_text(whole(15)), "Clicked 0 times");
    release(&mut window, 390.0, 10.0);

    // Clicking the button lays the label out again, still selectable.
    press(&mut window, 10.0, 10.0, 1, Modifiers::empty());
    release(&mut window, 10.0, 10.0);
    window.poll();
    assert!(window.layout(), "lays out");
    assert_eq!(window.selected_text(whole(15)), "Clicked 1 times");
}
