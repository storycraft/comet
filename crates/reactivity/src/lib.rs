//! Fine-grained reactive UI building on top of `reactive_graph`.
//!
//! Components are functions of a [`UiCx`] that build nodes once and return
//! them as a [`IntoView`], unattached; [`NodeBuild::children`] attaches the view
//! returned by its closure to its node. Props bound with
//! [`NodeBuild::style_with`] are written into the shared [`UiTree`] when the
//! signals they read change, recording what the change invalidates for the
//! application to take.
//!
//! [`UiCx`]: cx::UiCx
//! [`IntoView`]: view::IntoView
//! [`NodeBuild::children`]: cx::NodeBuild::children
//! [`NodeBuild::style_with`]: cx::NodeBuild::style_with
//! [`UiTree`]: tree::UiTree
//!
//! Setting up `reactive_graph` (the `any_spawner` executor and a root
//! [`Owner`](reactive_graph::owner::Owner)) and driving layout and paint are
//! left to the application.

pub mod cx;
pub mod prelude;
pub mod prop;
pub mod style;
pub mod tree;
pub mod view;
