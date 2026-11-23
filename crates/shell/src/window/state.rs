//! Window state exposed to components as signals.

use comet_tree::node::NodeKey;
use dpi::PhysicalSize;
use kurbo::Size;
use reactive_graph::{
    owner::use_context,
    signal::{ReadSignal, RwSignal, Trigger, WriteSignal, signal},
    traits::{GetUntracked, Set},
};

use crate::selection::Selection;

/// Window state as signals, for components to follow. Provided as context
/// to the window's components: see [`use_window`].
#[derive(Clone)]
pub struct WindowState {
    /// Size of the viewport, in logical units.
    pub size: ReadSignal<Size>,
    /// Size of the surface, in physical pixels.
    pub physical_size: ReadSignal<PhysicalSize<u32>>,
    pub scale_factor: ReadSignal<f64>,
    /// Whether the window has keyboard focus.
    pub focused: ReadSignal<bool>,
    /// Whether the window is hidden from view, such as when minimized.
    pub occluded: ReadSignal<bool>,
    /// Whether a pointer is over the window.
    pub pointer_inside: ReadSignal<bool>,
    /// Notified when the user asks to close the window.
    pub close_requested: Trigger,
    /// Keep the window open when the user asks to close it.
    pub prevent_close: RwSignal<bool>,
    /// The focused node. Set it to move the focus, which runs the focus
    /// handlers on the next [`Window::poll`](super::Window::poll).
    pub focused_node: RwSignal<Option<NodeKey>>,
    /// The text selection. Set it to change the selection.
    pub selection: RwSignal<Option<Selection>>,
}

/// The writing ends of the read-only signals of [`WindowState`], kept by
/// the window.
pub(super) struct StateWriters {
    pub size: WriteSignal<Size>,
    pub physical_size: WriteSignal<PhysicalSize<u32>>,
    pub scale_factor: WriteSignal<f64>,
    pub focused: WriteSignal<bool>,
    pub occluded: WriteSignal<bool>,
    pub pointer_inside: WriteSignal<bool>,
}

impl WindowState {
    pub(super) fn new() -> (Self, StateWriters) {
        let (size, set_size) = signal(Size::ZERO);
        let (physical_size, set_physical_size) = signal(PhysicalSize::new(0, 0));
        let (scale_factor, set_scale_factor) = signal(1.0);
        let (focused, set_focused) = signal(false);
        let (occluded, set_occluded) = signal(false);
        let (pointer_inside, set_pointer_inside) = signal(false);

        (
            Self {
                size,
                physical_size,
                scale_factor,
                focused,
                occluded,
                pointer_inside,
                close_requested: Trigger::new(),
                prevent_close: RwSignal::new(false),
                focused_node: RwSignal::new(None),
                selection: RwSignal::new(None),
            },
            StateWriters {
                size: set_size,
                physical_size: set_physical_size,
                scale_factor: set_scale_factor,
                focused: set_focused,
                occluded: set_occluded,
                pointer_inside: set_pointer_inside,
            },
        )
    }
}

/// The state of the window the current component is built in.
pub fn use_window() -> Option<WindowState> {
    use_context()
}

/// Set a signal only when the value changes, so followers are not woken
/// for nothing.
pub(super) fn update<T: Clone + PartialEq + Send + Sync + 'static>(
    read: ReadSignal<T>,
    write: WriteSignal<T>,
    value: T,
) {
    if read.get_untracked() != value {
        write.set(value);
    }
}

/// Set a writable signal only when the value changes.
pub(super) fn update_rw<T: Clone + PartialEq + Send + Sync + 'static>(
    signal: RwSignal<T>,
    value: T,
) {
    if signal.get_untracked() != value {
        signal.set(value);
    }
}
