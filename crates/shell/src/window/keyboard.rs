//! Keyboard input.

use ui_events::keyboard::{Key, KeyState, KeyboardEvent, Modifiers, NamedKey};

use super::Window;
use crate::style::{OnKeyDown, OnKeyUp};

impl Window {
    /// See [`Window::keyboard_event`].
    pub(super) fn handle_key(&mut self, event: &KeyboardEvent) {
        let target = self
            .focus
            .focused
            .filter(|&node| self.attached(node))
            .unwrap_or(self.ui.root());
        let path = self.ancestors(Some(target));

        let prevented = match event.state {
            KeyState::Down => self.dispatch_key::<OnKeyDown>(&path, event),
            KeyState::Up => self.dispatch_key::<OnKeyUp>(&path, event),
        };

        if !prevented && event.state == KeyState::Down && event.key == Key::Named(NamedKey::Tab) {
            self.move_focus(!event.modifiers.contains(Modifiers::SHIFT));
        }
    }
}
