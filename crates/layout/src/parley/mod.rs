//! Text style conversion and inheritance for Parley inline layout.
pub mod conv;
pub mod style;

use comet_tree::node::NodeKey;

pub type TextStyle = ::parley::TextStyle<'static, 'static, Option<NodeKey>>;

/// Default text style
pub fn default_text_style() -> TextStyle {
    TextStyle::default()
}
