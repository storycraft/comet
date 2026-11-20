//! Accessibility props of nodes.

use comet_tree::style::define_style_props;

define_style_props!(
    /// Role of the fragments a node generates.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Role: accesskit::Role = accesskit::Role::GenericContainer;
);
