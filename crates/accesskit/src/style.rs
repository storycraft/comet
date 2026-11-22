//! Accessibility props of nodes.

use comet_tree::style::define_style_props;

define_style_props!(
    /// Role of the fragments a node generates. Without it, texts are
    /// [labels](accesskit::Role::Label), boxes laying out inline content,
    /// such as text, without blocks splitting it,
    /// [groups](accesskit::Role::Group), and other boxes
    /// [generic containers](accesskit::Role::GenericContainer).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Role: accesskit::Role;
);
