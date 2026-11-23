//! A platform independent window showing a reactive UI.
//!
//! [`Window`] owns a UI built with [`comet_reactivity`] components and the
//! [pipeline](pipeline::Pipeline) laying it out and painting it. The
//! platform layer feeds it [`ui_events`] input and window events, and draws
//! the frames it asks for into an [`anyrender`] scene.
//!
//! Input is dispatched to the node under the pointer, or the focused node
//! for keyboard events, and its ancestors, calling the handlers registered
//! with the [`styles`] props. Window state is exposed to components as
//! signals through [`use_window`], and frame handlers are registered with
//! [`on_frame`].
//!
//! Signals are only expected to be set on the thread running the window.

pub mod event;
pub mod frame;
pub mod pipeline;
pub mod runtime;
pub mod styles;
pub mod window;

pub use accesskit;
pub use ui_events;

pub use event::{ClickEvent, EventCx, FocusEvent, Handler};
pub use frame::{on_frame, on_frame_while};
pub use runtime::UiRoot;
pub use window::{Window, WindowState, use_window};
