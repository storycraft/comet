//! Props for input handling.
//!
//! Handler props hold a [`Handler`], created under the owner of the
//! component setting it:
//!
//! ```ignore
//! cx.div()
//!     .style((OnClick(Handler::new(move |_, _| set_count.update(|n| *n += 1))),))
//!     .build()
//! ```
//!
//! Swapping a handler invalidates nothing, so handlers may also be bound
//! reactively with `style_with`: a handler created in a binding is dropped
//! when it runs again.

use comet_reactivity::{PropHint, ReactiveProp};
use comet_tree::style::define_style_props;

use crate::event::{
    ClickEvent, FocusEvent, Handler, KeyboardEvent, PointerButtonEvent, PointerInfo,
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
