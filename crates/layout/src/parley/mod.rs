//! Text style conversion and inheritance for Parley inline layout.
pub mod conv;
pub mod style;

use comet_tree::node::NodeKey;

use crate::style::parley::Alignment;

/// Parley style of a text span. Its brush identifies the source node.
pub type SpanStyle = ::parley::TextStyle<'static, 'static, Option<NodeKey>>;

/// Inherited text properties of an item: the style of its text spans and the
/// alignment of the lines in an inline formatting context it establishes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextStyle {
    pub span: SpanStyle,
    pub align: Alignment,
}

/// Default text style
pub fn default_text_style() -> TextStyle {
    TextStyle::default()
}
