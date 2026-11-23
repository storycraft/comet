//! Showing the window's mouse cursor with winit.

use comet_shell::CursorIcon;
use winit::window::{CursorIcon as Icon, Window};

/// Show `cursor` over `win`, or hide the cursor with `None`.
pub(crate) fn set_cursor(win: &Window, cursor: Option<CursorIcon>) {
    match cursor {
        Some(cursor) => {
            win.set_cursor_visible(true);
            win.set_cursor(winit_icon(cursor));
        }
        None => win.set_cursor_visible(false),
    }
}

/// The winit icon of `cursor`.
fn winit_icon(cursor: CursorIcon) -> Icon {
    match cursor {
        CursorIcon::Default => Icon::Default,
        CursorIcon::ContextMenu => Icon::ContextMenu,
        CursorIcon::Help => Icon::Help,
        CursorIcon::Pointer => Icon::Pointer,
        CursorIcon::Progress => Icon::Progress,
        CursorIcon::Wait => Icon::Wait,
        CursorIcon::Cell => Icon::Cell,
        CursorIcon::Crosshair => Icon::Crosshair,
        CursorIcon::Text => Icon::Text,
        CursorIcon::VerticalText => Icon::VerticalText,
        CursorIcon::Alias => Icon::Alias,
        CursorIcon::Copy => Icon::Copy,
        CursorIcon::Move => Icon::Move,
        CursorIcon::NoDrop => Icon::NoDrop,
        CursorIcon::NotAllowed => Icon::NotAllowed,
        CursorIcon::Grab => Icon::Grab,
        CursorIcon::Grabbing => Icon::Grabbing,
        CursorIcon::AllScroll => Icon::AllScroll,
        CursorIcon::ColResize => Icon::ColResize,
        CursorIcon::RowResize => Icon::RowResize,
        CursorIcon::NResize => Icon::NResize,
        CursorIcon::EResize => Icon::EResize,
        CursorIcon::SResize => Icon::SResize,
        CursorIcon::WResize => Icon::WResize,
        CursorIcon::NeResize => Icon::NeResize,
        CursorIcon::NwResize => Icon::NwResize,
        CursorIcon::SeResize => Icon::SeResize,
        CursorIcon::SwResize => Icon::SwResize,
        CursorIcon::EwResize => Icon::EwResize,
        CursorIcon::NsResize => Icon::NsResize,
        CursorIcon::NeswResize => Icon::NeswResize,
        CursorIcon::NwseResize => Icon::NwseResize,
        CursorIcon::ZoomIn => Icon::ZoomIn,
        CursorIcon::ZoomOut => Icon::ZoomOut,
    }
}
