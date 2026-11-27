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
//! signals through [`use_window`]. Window-wide input and frame listeners
//! are registered with the [`listeners`] functions, such as
//! [`on_window_key`] and [`on_frame`].
//!
//! Signals are only expected to be set on the thread running the window.
//!
//! [`Window`]: window::Window
//! [`use_window`]: window::state::use_window
//! [`on_window_key`]: listeners::on_window_key
//! [`on_frame`]: listeners::on_frame

pub mod cursor;
pub mod event;
pub mod listeners;
pub mod pipeline;
pub mod runtime;
pub mod selection;
pub mod style;
pub mod window;
