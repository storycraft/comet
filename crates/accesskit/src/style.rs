//! Accessibility props of nodes.

use comet_tree::style::define_style_props;

define_style_props!(
    /// Role of the node of a box. Without it, boxes laying out inline
    /// content, such as text, without blocks splitting it, are
    /// [groups](accesskit::Role::Group), and other boxes
    /// [generic containers](accesskit::Role::GenericContainer). Text nodes
    /// cannot have one: they are always [labels](accesskit::Role::Label),
    /// and a role set on them is ignored.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Role: accesskit::Role;
);
