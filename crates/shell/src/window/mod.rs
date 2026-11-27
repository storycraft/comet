//! A platform independent window showing a reactive UI.
//!
//! [`Window`] keeps its state in groups, each in a submodule with the
//! methods working on it, and wraps them in its public methods.

mod dispatch;
mod focus;
mod highlight;
mod keyboard;
mod pointer;
mod selection;
pub mod state;
pub mod text;
mod viewport;

use core::iter::successors;
use std::time::Instant;

use accesskit::{Action, ActionRequest, TreeUpdate};
use comet_geometry::AutoValue;
use comet_reactivity::cx::UiCx;
use comet_tree::{node::NodeKey, style::StyleProp};
use dpi::PhysicalSize;
use kurbo::Size;
use reactive_graph::{
    owner::{Owner, provide_context},
    traits::{GetUntracked, Notify},
};
use ui_events::{keyboard::KeyboardEvent, pointer::PointerEvent};

use self::{
    focus::FocusState,
    pointer::PointerState,
    selection::SelectionState,
    state::{StateWriters, WindowState, update},
    viewport::Viewport,
};
use comet_paint::selection::Selection;

use crate::{cursor::CursorIcon, listeners::Listeners, pipeline::Pipeline, runtime::UiRoot};

/// A window showing a UI built with [`comet_reactivity`], independent of the
/// platform.
///
/// The platform layer feeds it input and window events, polls it after
/// each batch of events, and draws a frame when it
/// [needs one](Self::needs_redraw):
///
/// 1. [`layout`](Self::layout), which runs the frame listeners first,
/// 2. [`take_accessibility_update`](Self::take_accessibility_update) for the
///    platform's accessibility adapter,
/// 3. [`paint`](Self::paint) into the platform's scene.
pub struct Window {
    ui: UiRoot,
    pipeline: Pipeline,
    state: WindowState,
    writers: StateWriters,
    listeners: Listeners,
    viewport: Viewport,
    pointer: PointerState,
    focus: FocusState,
    selection: SelectionState,
    /// Origin of frame times.
    start: Instant,
    /// Whether something not recorded in the tree needs a frame.
    redraw: bool,
    /// Owns the window's context and signals. Dropped after the UI.
    owner: Owner,
}

impl Window {
    /// Build the UI with the `root` component, under a new owner providing
    /// the window's [`WindowState`] as context. The node it returns is the
    /// root of the layout.
    pub fn new(root: impl FnOnce(&UiCx) -> NodeKey) -> Self {
        let owner = Owner::new();
        let (state, writers, listeners) = owner.with(|| {
            let (state, writers) = WindowState::new();
            let listeners = Listeners::new();
            provide_context(state.clone());
            provide_context(listeners);

            (state, writers, listeners)
        });
        let ui = owner.with(|| UiRoot::new(root));

        Self {
            ui,
            pipeline: Pipeline::default(),
            state,
            writers,
            listeners,
            viewport: Viewport::default(),
            pointer: PointerState::default(),
            focus: FocusState::default(),
            selection: SelectionState::default(),
            start: Instant::now(),
            redraw: true,
            owner,
        }
    }

    pub fn ui(&self) -> &UiRoot {
        &self.ui
    }

    /// The node built by the root component.
    pub fn root(&self) -> NodeKey {
        self.ui.root()
    }

    pub fn state(&self) -> &WindowState {
        &self.state
    }

    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    pub fn pipeline(&self) -> &Pipeline {
        &self.pipeline
    }

    pub fn pipeline_mut(&mut self) -> &mut Pipeline {
        &mut self.pipeline
    }

    /// The mouse cursor to show, `None` hiding it, also followed by
    /// [`WindowState::cursor`].
    pub fn cursor(&self) -> Option<CursorIcon> {
        self.state.cursor.get_untracked()
    }

    pub fn selection(&self) -> Option<Selection> {
        self.selection.current
    }

    pub fn focused(&self) -> Option<NodeKey> {
        self.focus.focused
    }

    pub fn physical_size(&self) -> PhysicalSize<u32> {
        self.viewport.physical_size
    }

    pub fn scale_factor(&self) -> f64 {
        self.viewport.scale_factor
    }

    /// Size of the viewport, in logical units: the surface divided by the
    /// scale factor.
    pub fn size(&self) -> Size {
        self.viewport.size()
    }

    // Window events.

    /// The surface was resized, or moved to a display with another scale
    /// factor.
    pub fn resize(&mut self, physical_size: PhysicalSize<u32>, scale_factor: f64) {
        self.viewport = Viewport {
            physical_size,
            scale_factor,
        };

        let (state, writers) = (&self.state, &self.writers);
        update(state.physical_size, writers.physical_size, physical_size);
        update(state.scale_factor, writers.scale_factor, scale_factor);
        update(state.size, writers.size, self.viewport.size());
        self.redraw = true;
    }

    /// The window gained or lost keyboard focus.
    pub fn set_focused(&mut self, focused: bool) {
        update(self.state.focused, self.writers.focused, focused);
    }

    pub fn set_occluded(&mut self, occluded: bool) {
        update(self.state.occluded, self.writers.occluded, occluded);
        if !occluded {
            self.redraw = true;
        }
    }

    /// The user asked to close the window. Returns whether to close it,
    /// unless [`WindowState::prevent_close`] is set.
    pub fn request_close(&mut self) -> bool {
        self.state.close_requested.notify();

        !self.state.prevent_close.get_untracked()
    }

    // Frames.

    /// Run the bindings woken by the events so far, and apply focus and
    /// selection changes made through [`WindowState::focused_node`] and
    /// [`WindowState::selection`]. Call it after each batch of events,
    /// without borrows of the tree.
    pub fn poll(&mut self) {
        self.ui.poll();

        self.sync_focus();
        // A captured node removed from the tree.
        if self
            .pointer
            .capture
            .is_some_and(|node| !self.attached(node))
        {
            self.pointer.capture = None;
        }
        self.sync_selection();
        // Cursors bound to signals change without the pointer moving.
        self.update_cursor();

        self.ui.poll();
    }

    /// Whether a frame should be drawn: the tree changed, something
    /// animates, or the window changed.
    pub fn needs_redraw(&self) -> bool {
        self.redraw || !self.ui.tree().borrow().changes().is_empty() || self.listeners.animating()
    }

    /// Run the frame handlers, then lay out the UI at the current size.
    /// Returns whether there is a layout to paint.
    pub fn layout(&mut self) -> bool {
        self.redraw = false;
        // This frame shows every change so far. Changes are taken even
        // without a frame, so they do not ask for one again.
        self.pipeline.take_changes(&mut self.ui.tree().borrow_mut());

        if self.viewport.is_empty() {
            return false;
        }

        // Frame listeners set signals: run the bindings they woke before
        // laying out.
        self.listeners.run_frame(self.start.elapsed());
        self.ui.poll();

        let size = self.viewport.size();
        let viewport = comet_geometry::Size::new(size.width, size.height);
        let laid_out = self
            .pipeline
            .layout(&mut self.ui.tree().borrow_mut(), self.ui.root(), viewport)
            .is_some();

        // A node focused before it was laid out is revealed now. The scroll
        // is painted in the next frame, which the change requests.
        if laid_out {
            self.reveal_focus();
        }

        laid_out
    }

    /// Paint the last layout into `scene`, in physical pixels, with the
    /// selected text highlighted behind its glyphs.
    pub fn paint(&mut self, scene: &mut impl anyrender::PaintScene) {
        let tree = self.ui.tree().borrow();
        self.pipeline.paint(
            &tree.ui,
            &tree.contents,
            self.viewport.physical_rect(),
            self.viewport.scale_factor,
            scene,
        );
    }

    // Accessibility.

    /// Start or stop building the accessibility tree, such as when an
    /// assistive technology connects. Starting builds the whole tree with
    /// the next frame, which it requests.
    pub fn set_accessibility(&mut self, active: bool) {
        self.pipeline.set_accessibility(active);
        self.redraw |= active;
    }

    /// The accessibility changes since the last update, with the focus.
    /// `None` while inactive or without changes.
    pub fn take_accessibility_update(&mut self) -> Option<TreeUpdate> {
        let tree = self.ui.tree().borrow();

        self.pipeline.take_accessibility_update(
            &tree.ui,
            self.viewport.scale_factor,
            self.focus.focused,
        )
    }

    /// Perform an action requested by an assistive technology.
    pub fn accessibility_action(&mut self, request: &ActionRequest) {
        let Some(node) = self.pipeline.accessibility_target(request.target_node) else {
            return;
        };

        match request.action {
            Action::Click => self.click(node),
            Action::Focus => self.set_focus(Some(node)),
            Action::Blur if self.focus.focused == Some(node) => self.set_focus(None),
            _ => {}
        }
    }

    // Input.

    /// Dispatch a pointer event to the node under the pointer, or the node
    /// capturing it, and its ancestors, between the capturing and bubbling
    /// window listeners, then perform its default action: focusing and
    /// selecting text on press, extending the selection while dragging, and
    /// scrolling on wheel. Window listeners also get the events nodes do
    /// not: pointer enter, leave, cancel and gestures.
    pub fn pointer_event(&mut self, event: &PointerEvent) {
        self.handle_pointer(event);
    }

    /// Dispatch a keyboard event to the focused node, or the root without
    /// focus, and its ancestors, between the capturing and bubbling window
    /// listeners, then perform its default action: Tab moves the focus,
    /// starting from the extent of the selection without focus.
    pub fn keyboard_event(&mut self, event: &KeyboardEvent) {
        self.handle_key(event);
    }

    /// `node` and its ancestors, innermost first.
    fn ancestors(&self, node: Option<NodeKey>) -> Vec<NodeKey> {
        let tree = self.ui.tree().borrow();

        successors(node, |&node| tree.ui.parent(node)).collect()
    }

    /// The value of an inherited prop `P` for `node`: from it up the tree,
    /// the first node whose prop, read by `value`, is not
    /// [`AutoValue::Auto`]. `None` when no node decides.
    fn inherited<P: StyleProp, T>(
        &self,
        node: Option<NodeKey>,
        value: impl Fn(&P) -> AutoValue<T>,
    ) -> Option<T> {
        let tree = self.ui.tree().borrow();

        successors(node, |&node| tree.ui.parent(node)).find_map(|node| {
            match tree.ui.prop::<P>(node).map(|prop| value(&prop)) {
                Some(AutoValue::Value(value)) => Some(value),
                _ => None,
            }
        })
    }

    /// Whether `node` is in the window's tree.
    fn attached(&self, node: NodeKey) -> bool {
        let root = self.ui.root();
        let tree = self.ui.tree().borrow();

        successors(Some(node), |&node| tree.ui.parent(node)).any(|node| node == root)
    }
}
