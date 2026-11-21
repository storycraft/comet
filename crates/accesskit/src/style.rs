//! Accessibility props of nodes.

use comet_tree::style::define_style_props;

define_style_props!(
    /// Role of the fragments a node generates. Without it, texts are
    /// [labels](accesskit::Role::Label) and boxes
    /// [generic containers](accesskit::Role::GenericContainer). Glyph runs
    /// are always [text runs](accesskit::Role::TextRun).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub Role: accesskit::Role;
);
