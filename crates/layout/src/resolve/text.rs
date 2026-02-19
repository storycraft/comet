use comet_div::{
    style_prop,
    ui::{NodeKey, Ui},
};
use kurbo::Size;
use parley::TextStyle;

use crate::{
    parley::default_text_style,
    style::{FontSize, LayoutStyleCx},
};

struct ResolvedTextStyle(TextStyle<'static, ()>);
style_prop!(ResolvedTextStyle);

pub fn resolve_text_style(ui: &mut Ui, id: NodeKey, root_size: Size) -> TextStyle<'static, ()> {
    if let Some(resolved) = ui.prop::<ResolvedTextStyle>(id) {
        return resolved.0.clone();
    }

    let mut style = if let Some(parent_id) = ui.parent(id) {
        resolve_text_style(ui, parent_id, root_size)
    } else {
        default_text_style(())
    };
    let props = ui.props(id).unwrap();

    // TODO:: proper style cx
    let cx = LayoutStyleCx {
        root_size,
        root_font_size: 16.0,
        parent_size: Size::new(1920.0, 1080.0),
        parent_font_size: style.font_size as _,
    };

    // TODO:: apply styles
    if let Some(size) = props.get::<FontSize>() {
        style.font_size = size.0.resolve(&cx) as _;
    }

    ui.set_props(id, (ResolvedTextStyle(style.clone()),));
    style
}

pub fn invalidate_text_style(ui: &mut Ui, id: NodeKey) {
    ui.remove_prop::<ResolvedTextStyle>(id);
}
