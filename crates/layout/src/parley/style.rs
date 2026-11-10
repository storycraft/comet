use super::{TextStyle, default_text_style};

/// Default style for a root without a source parent. Its font size is the root font size.
pub fn root_text_style(root_font_size: f64) -> TextStyle {
    TextStyle {
        font_size: root_font_size as f32,
        ..default_text_style()
    }
}
