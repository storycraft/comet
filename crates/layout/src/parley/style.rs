use super::{SpanStyle, TextStyle};

/// Default style for a root without a source parent. Its font size is the root font size.
pub fn root_text_style(root_font_size: f64) -> TextStyle {
    TextStyle {
        span: SpanStyle {
            font_size: root_font_size as f32,
            ..SpanStyle::default()
        },
        ..TextStyle::default()
    }
}
