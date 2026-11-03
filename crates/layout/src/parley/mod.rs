//! Text style conversion and inheritance for Parley inline layout.
pub mod conv;
pub mod style;

use comet_tree::node::NodeKey;

pub type TextStyle = ::parley::TextStyle<'static, 'static, Option<NodeKey>>;

/// CSS initial values: medium (16px), serif, normal line height, collapsed whitespace.
pub fn default_text_style() -> TextStyle {
    TextStyle {
        font_family: ::parley::GenericFamily::Serif.into(),
        white_space_collapse: ::parley::WhiteSpaceCollapse::Collapse,
        ..Default::default()
    }
}
