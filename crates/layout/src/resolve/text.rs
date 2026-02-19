use comet_div::{
    style_prop,
    ui::{NodeKey, Ui},
};
use kurbo::Size;
use parley::TextStyle;

use crate::{parley::default_text_style, style::*};

struct ResolvedTextStyle(TextStyle<'static, Option<NodeKey>>);
style_prop!(ResolvedTextStyle);

pub fn resolve_text_style(ui: &mut Ui, id: NodeKey, root_size: Size) -> TextStyle<'static, Option<NodeKey>> {
    if let Some(resolved) = ui.prop::<ResolvedTextStyle>(id) {
        return resolved.0.clone();
    }

    let mut style = if let Some(parent_id) = ui.parent(id) {
        resolve_text_style(ui, parent_id, root_size)
    } else {
        default_text_style(None)
    };
    style.brush = Some(id);
    let props = ui.props(id).unwrap();

    // TODO:: proper style cx
    let cx = LayoutStyleCx {
        root_size,
        root_font_size: 16.0,
        parent_size: Size::new(style.font_size as _, style.font_size as _),
        parent_font_size: style.font_size as _,
    };

    if let Some(v) = props.get::<Font>() {
        style.font_stack = v.0.clone();
    }
    if let Some(v) = props.get::<FontSize>() {
        style.font_size = v.0.resolve(&cx) as _;
    }
    if let Some(v) = props.get::<FontVariations>() {
        style.font_variations = v.0.clone();
    }
    if let Some(v) = props.get::<FontFeatures>() {
        style.font_features = v.0.clone();
    }
    if let Some(v) = props.get::<FontStyle1>() {
        style.font_style = v.0.clone();
    }
    if let Some(v) = props.get::<FontWeight1>() {
        style.font_weight = v.0.clone();
    }
    if let Some(v) = props.get::<Locale>() {
        style.locale = Some(v.0);
    }

    if let Some(v) = props.get::<UnderlineOffset>() {
        style.underline_offset = Some(v.0.resolve(&cx) as _);
    }
    if let Some(v) = props.get::<UnderlineSize>() {
        style.underline_size = Some(v.0.resolve(&cx) as _);
    }

    if let Some(v) = props.get::<StrikethroughOffset>() {
        style.strikethrough_offset = Some(v.0.resolve(&cx) as _);
    }
    if let Some(v) = props.get::<StrikethroughSize>() {
        style.strikethrough_size = Some(v.0.resolve(&cx) as _);
    }

    if let Some(v) = props.get::<LineHeight>() {
        style.line_height = parley::LineHeight::Absolute(v.0.resolve(&cx) as _);
    }
    if let Some(v) = props.get::<WordBreak>() {
        style.word_break = v.0;
    }
    if let Some(v) = props.get::<WordSpacing>() {
        style.word_spacing = v.0.resolve(&cx) as _;
    }
    if let Some(v) = props.get::<LetterSpacing>() {
        style.letter_spacing = v.0.resolve(&cx) as _;
    }

    ui.set_props(id, (ResolvedTextStyle(style.clone()),));
    style
}

pub fn invalidate_text_style(ui: &mut Ui, id: NodeKey) {
    ui.remove_prop::<ResolvedTextStyle>(id);
}
