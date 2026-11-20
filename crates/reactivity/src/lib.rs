//! Fine-grained reactive UI building on top of `reactive_graph`.
//!
//! Components are functions of a [`UiCx`] that build nodes once and return
//! a [`NodeKey`](comet_tree::node::NodeKey). Props bound with
//! [`NodeBuild::style_with`] are written into the shared [`UiTree`] when the
//! signals they read change, recording what the change invalidates for the
//! application to take.
//!
//! Setting up `reactive_graph` (the `any_spawner` executor and a root
//! [`Owner`](reactive_graph::owner::Owner)) and driving layout and paint are
//! left to the application.

pub mod cx;
pub mod prelude;
pub mod prop;
pub mod style;
pub mod tree;

pub use cx::{NodeBuild, UiCx};
pub use prop::{PropHint, ReactiveProp};
pub use style::{FlexStyled, Styled};
pub use tree::UiTree;
