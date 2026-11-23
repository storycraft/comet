//! Props for input handling, and [`InputStyled`] helpers setting them.
//!
//! Handler props hold a [`Handler`], created under the owner of the
//! component setting it. The helpers create it from a closure:
//!
//! ```ignore
//! cx.div()
//!     .focusable(true)
//!     .on_click(move |_, _| set_count.update(|n| *n += 1))
//!     .build()
//! ```
//!
//! Swapping a handler invalidates nothing, so handlers may also be bound
//! reactively with `style_with`: a handler created in a binding is dropped
//! when it runs again.

use core::cell::RefCell;

use comet_layout::style::geometry::AutoValue;
use comet_reactivity::{PropHint, ReactiveProp, Styled};
use comet_tree::style::define_style_props;

use crate::cursor::CursorIcon;
use reactive_graph::wrappers::read::Signal;

use crate::event::{
    ClickEvent, EventCx, FocusEvent, Handler, KeyboardEvent, PointerButtonEvent, PointerInfo,
    PointerScrollEvent, PointerUpdate,
};

define_style_props!(
    /// A pointer button was pressed over the node or a descendant.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnPointerDown: Handler<PointerButtonEvent>;
    /// A pointer button was released over the node or a descendant, or
    /// while the node captures the pointer.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnPointerUp: Handler<PointerButtonEvent>;
    /// A pointer moved over the node or a descendant, or while the node
    /// captures the pointer.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnPointerMove: Handler<PointerUpdate>;
    /// A pointer entered the node and its descendants. Not propagated.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnPointerEnter: Handler<PointerInfo>;
    /// A pointer left the node and its descendants. Not propagated.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnPointerLeave: Handler<PointerInfo>;
    /// A wheel or touchpad scrolled over the node or a descendant. The
    /// default action scrolls the innermost scroll container.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnScroll: Handler<PointerScrollEvent>;
    /// The node or a descendant was clicked.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnClick: Handler<ClickEvent>;
    /// A key was pressed while the node or a descendant has focus.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnKeyDown: Handler<KeyboardEvent>;
    /// A key was released while the node or a descendant has focus.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnKeyUp: Handler<KeyboardEvent>;
    /// The node gained focus. Not propagated.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnFocus: Handler<FocusEvent>;
    /// The node lost focus. Not propagated.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub OnBlur: Handler<FocusEvent>;

    /// Whether the node can be focused, by a press or with Tab, in tree
    /// order.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Focusable: bool = false;

    /// Whether pressing the text of the node and its descendants selects
    /// it. Inherited: the nearest node with the prop decides. Text is
    /// selectable without one.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Selectable: bool;

    /// The mouse cursor shown over the node and its descendants, or none
    /// to hide it.
    ///
    /// Resolved from the node under the pointer up: the nearest node with a
    /// cursor other than [`AutoValue::Auto`] decides, and the default cursor
    /// shows when none does. Without the prop, text nodes show
    /// [`CursorIcon::Text`] and boxes are `Auto`.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Cursor: AutoValue<Option<CursorIcon>>;
);

/// A prop holding a [`Handler`].
pub trait HandlerProp: ReactiveProp + Copy {
    type Event: 'static;

    fn handler(self) -> Handler<Self::Event>;
}

macro_rules! handler_props {
    ($($prop:ty => $event:ty),* $(,)?) => {$(
        impl ReactiveProp for $prop {
            const HINT: PropHint = PropHint::empty();
        }

        impl HandlerProp for $prop {
            type Event = $event;

            fn handler(self) -> Handler<$event> {
                self.0
            }
        }
    )*};
}

handler_props!(
    OnPointerDown => PointerButtonEvent,
    OnPointerUp => PointerButtonEvent,
    OnPointerMove => PointerUpdate,
    OnPointerEnter => PointerInfo,
    OnPointerLeave => PointerInfo,
    OnScroll => PointerScrollEvent,
    OnClick => ClickEvent,
    OnKeyDown => KeyboardEvent,
    OnKeyUp => KeyboardEvent,
    OnFocus => FocusEvent,
    OnBlur => FocusEvent,
);

impl ReactiveProp for Focusable {
    const HINT: PropHint = PropHint::empty();
}

impl ReactiveProp for Selectable {
    const HINT: PropHint = PropHint::empty();
}

impl ReactiveProp for Cursor {
    const HINT: PropHint = PropHint::empty();
}

/// Generate helpers setting the handler prop `$prop` to a new [`Handler`].
macro_rules! handler_helpers {
    ($($(#[$attr:meta])* $name:ident: $prop:ty => $event:ty;)*) => {$(
        $(#[$attr])*
        #[track_caller]
        fn $name(self, handler: impl FnMut(&mut EventCx, &$event) + 'static) -> Self {
            self.bind::<$prop, Handler<$event>>(Handler::new(handler))
        }
    )*};
}

/// Input helpers of a node being built: handlers of the [`HandlerProp`]
/// props, [`Focusable`], [`Selectable`] and [`Cursor`].
///
/// Each handler is created under the current owner, the owner of the
/// component building the node, and runs under it. Setting a handler again
/// replaces the earlier one.
pub trait InputStyled: Styled {
    /// Let the node be focused, by a press or with Tab, in tree order.
    fn focusable(self, value: impl Into<Signal<bool>>) -> Self {
        self.bind::<Focusable, bool>(value)
    }

    /// Let a press select the text of the node and its descendants, which
    /// it does without the prop.
    fn selectable(self, value: impl Into<Signal<bool>>) -> Self {
        self.bind::<Selectable, bool>(value)
    }

    /// Show `value` over the node and its descendants, hide the cursor with
    /// `None`, or let the ancestors decide with [`AutoValue::Auto`].
    fn cursor(self, value: impl Into<Signal<AutoValue<Option<CursorIcon>>>>) -> Self {
        self.bind::<Cursor, AutoValue<Option<CursorIcon>>>(value)
    }

    handler_helpers! {
        /// Call `handler` when a pointer button is pressed over the node or
        /// a descendant.
        on_pointer_down: OnPointerDown => PointerButtonEvent;
        /// Call `handler` when a pointer button is released over the node or
        /// a descendant, or while the node captures the pointer.
        on_pointer_up: OnPointerUp => PointerButtonEvent;
        /// Call `handler` when a pointer moves over the node or a
        /// descendant, or while the node captures the pointer.
        on_pointer_move: OnPointerMove => PointerUpdate;
        /// Call `handler` when a pointer enters the node and its
        /// descendants.
        on_pointer_enter: OnPointerEnter => PointerInfo;
        /// Call `handler` when a pointer leaves the node and its
        /// descendants.
        on_pointer_leave: OnPointerLeave => PointerInfo;
        /// Call `handler` when a wheel or touchpad scrolls over the node or
        /// a descendant.
        on_scroll: OnScroll => PointerScrollEvent;
        /// Call `handler` when the node or a descendant is clicked.
        on_click: OnClick => ClickEvent;
        /// Call `handler` when a key is pressed while the node or a
        /// descendant has focus.
        on_key_down: OnKeyDown => KeyboardEvent;
        /// Call `handler` when a key is released while the node or a
        /// descendant has focus.
        on_key_up: OnKeyUp => KeyboardEvent;
        /// Call `handler` when the node gains focus.
        on_focus: OnFocus => FocusEvent;
        /// Call `handler` when the node loses focus.
        on_blur: OnBlur => FocusEvent;
    }

    /// Call `handler` with `true` when a pointer enters the node and its
    /// descendants, and `false` when it leaves.
    #[track_caller]
    fn on_hover(self, handler: impl FnMut(bool) + 'static) -> Self {
        let handler = std::rc::Rc::new(RefCell::new(handler));
        let leave = handler.clone();

        self.on_pointer_enter(move |_, _| handler.borrow_mut()(true))
            .on_pointer_leave(move |_, _| leave.borrow_mut()(false))
    }
}

impl<S: Styled> InputStyled for S {}
