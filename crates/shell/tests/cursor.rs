//! The mouse cursor resolved from the node under the pointer.

use std::{cell::RefCell, rc::Rc};

use comet_layout::style::{geometry::AutoValue, unit::*};
use comet_reactivity::{Styled, UiCx};
use comet_shell::{
    CursorIcon, Window,
    style::InputStyled,
    ui_events::pointer::{
        PointerEvent, PointerId, PointerInfo, PointerState, PointerType, PointerUpdate,
    },
};
use dpi::{PhysicalPosition, PhysicalSize};
use reactive_graph::{
    signal::{WriteSignal, signal},
    traits::{GetUntracked, Set},
};

/// Sets the cursor of the box bound to a signal, once the window is built.
type Setter = Rc<RefCell<Option<WriteSignal<AutoValue<Option<CursorIcon>>>>>>;

const MOUSE: PointerInfo = PointerInfo {
    pointer_id: Some(PointerId::PRIMARY),
    persistent_device_id: None,
    pointer_type: PointerType::Mouse,
};

fn hover(window: &mut Window, x: f64, y: f64) -> Option<CursorIcon> {
    window.pointer_event(&PointerEvent::Move(PointerUpdate {
        pointer: MOUSE,
        current: PointerState {
            position: PhysicalPosition::new(x, y),
            ..Default::default()
        },
        coalesced: Vec::new(),
        predicted: Vec::new(),
    }));

    window.cursor()
}

/// A box filling a 40 high row.
fn row(cx: &UiCx) -> comet_reactivity::NodeBuild<'_> {
    cx.div().size(size(px(400), px(40)))
}

fn open(window: &mut Window) {
    window.resize(PhysicalSize::new(400, 300), 1.0);
    window.poll();
    assert!(window.layout(), "lays out");
}

/// From the top, 40 high each:
/// - a plain box,
/// - a box with text at its top-left,
/// - a box showing `Pointer`, holding a box showing `Auto` over its left
///   half, a box showing `Crosshair` over its right half, and below them, a
///   box without a cursor holding text showing `Grab` and text without one,
/// - a box whose cursor follows a signal, starting at `Auto`.
fn window() -> (Window, Setter) {
    let setter = Rc::new(RefCell::new(None));
    let window = Window::new({
        let setter = setter.clone();
        move |cx: &UiCx| {
            let (cursor, set_cursor) = signal(AutoValue::Auto);
            *setter.borrow_mut() = Some(set_cursor);

            let plain = row(cx).build();
            let text = row(cx).children(|cx| cx.text("Text").build());
            let pointer = cx
                .div()
                // 80 high with 40 of padding above its content; positioned,
                // so it places its absolute children.
                .size(size(px(400), px(80)))
                .padding(sides(px(40), px(0), px(0), px(0)))
                .position(comet_layout::style::taffy::Position::Relative)
                .cursor(AutoValue::Value(Some(CursorIcon::Pointer)))
                .children(|cx| {
                    let inherit = cx
                        .div()
                        .size(size(px(200), px(40)))
                        .position(comet_layout::style::taffy::Position::Absolute)
                        .cursor(AutoValue::Auto)
                        .build();
                    let crosshair = cx
                        .div()
                        .size(size(px(200), px(40)))
                        .position(comet_layout::style::taffy::Position::Absolute)
                        .inset(sides(px(0), auto(), auto(), px(200)))
                        .cursor(AutoValue::Value(Some(CursorIcon::Crosshair)))
                        .build();
                    let labels = cx.div().children(|cx| {
                        [
                            cx.text("Grab")
                                .cursor(AutoValue::Value(Some(CursorIcon::Grab)))
                                .build(),
                            cx.text(" label").build(),
                        ]
                    });

                    [inherit, crosshair, labels]
                });
            let bound = row(cx).cursor(cursor).build();

            cx.div().children(|_| [plain, text, pointer, bound])
        }
    });

    (window, setter)
}

#[test]
fn boxes_show_the_default_cursor_and_text_the_text_cursor() {
    let (mut window, _) = window();
    open(&mut window);

    assert_eq!(
        window.cursor(),
        Some(CursorIcon::Default),
        "before any pointer"
    );
    assert_eq!(
        hover(&mut window, 300.0, 20.0),
        Some(CursorIcon::Default),
        "a plain box"
    );
    assert_eq!(
        hover(&mut window, 2.0, 50.0),
        Some(CursorIcon::Text),
        "text"
    );
    assert_eq!(
        hover(&mut window, 300.0, 50.0),
        Some(CursorIcon::Default),
        "beside the text"
    );
}

#[test]
fn auto_keeps_the_cursor_above_and_boxes_show_the_default() {
    let (mut window, _) = window();
    open(&mut window);

    assert_eq!(
        hover(&mut window, 10.0, 90.0),
        Some(CursorIcon::Pointer),
        "inherited"
    );
    assert_eq!(
        hover(&mut window, 300.0, 90.0),
        Some(CursorIcon::Crosshair),
        "its own"
    );
    assert_eq!(
        hover(&mut window, 300.0, 150.0),
        Some(CursorIcon::Pointer),
        "the box itself"
    );
    assert_eq!(
        hover(&mut window, 300.0, 125.0),
        Some(CursorIcon::Default),
        "a box without a cursor, not inheriting"
    );
}

#[test]
fn text_shows_its_own_cursor_or_the_text_cursor() {
    let (mut window, _) = window();
    open(&mut window);

    assert_eq!(
        hover(&mut window, 2.0, 130.0),
        Some(CursorIcon::Grab),
        "its own"
    );
    // Past "Grab", over " label".
    assert_eq!(
        hover(&mut window, 60.0, 130.0),
        Some(CursorIcon::Text),
        "the text cursor, not the box's"
    );
}

#[test]
fn cursor_follows_props_and_the_window_signal() {
    let (mut window, setter) = window();
    open(&mut window);

    assert_eq!(
        hover(&mut window, 10.0, 180.0),
        Some(CursorIcon::Default),
        "auto"
    );

    setter
        .borrow()
        .unwrap()
        .set(AutoValue::Value(Some(CursorIcon::Wait)));
    window.poll();
    assert_eq!(
        window.cursor(),
        Some(CursorIcon::Wait),
        "without moving the pointer"
    );
    assert_eq!(
        window.state().cursor.get_untracked(),
        Some(CursorIcon::Wait)
    );

    setter.borrow().unwrap().set(AutoValue::Value(None));
    window.poll();
    assert_eq!(window.cursor(), None, "hidden");

    window.pointer_event(&PointerEvent::Leave(MOUSE));
    assert_eq!(
        window.cursor(),
        Some(CursorIcon::Default),
        "outside the window"
    );
}
