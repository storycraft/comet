//! Reactive UI in a window.
//!
//! Build the UI with [`comet_reactivity`] components and show it with
//! [`run`]. Bindings re-run on the event loop thread after every batch of
//! window events and before every frame; changed props are laid out and
//! painted again in the next frame. Input and frame handlers are registered
//! with [`Interactive`] and [`on_frame`].
//!
//! While an assistive technology is active, the window exposes an
//! accessibility tree mirroring the layout through `accesskit_winit`.
//! Click actions call the click handlers of the target and its ancestors.
//!
//! Signals are only expected to be set on the event loop thread, from
//! handlers and bindings. Signals set from other threads do not wake the
//! event loop.

pub mod event;
mod pipeline;
pub mod prelude;
pub mod runtime;

use core::error::Error;
use std::{sync::Arc, time::Instant};

use accesskit::Action;
use accesskit_winit::{Adapter, WindowEvent as AccessKitEvent};
use anyrender::WindowRenderer;
use anyrender_vello_gpu::VelloGpuWindowRenderer;
use comet_layout::style::geometry::Size;
use comet_reactivity::UiCx;
use comet_tree::{node::NodeKey, ui::Ui};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window, WindowAttributes, WindowId},
};

pub use accesskit;
pub use comet_reactivity as reactivity;
pub use event::{Interactive, on_frame, on_frame_while};
pub use reactive_graph;
pub use runtime::UiRoot;
pub use winit;

use crate::pipeline::Pipeline;

/// Build the UI with `root` and show it in a window opened with
/// `attributes`, until the window is closed.
pub fn run(
    attributes: WindowAttributes,
    root: impl FnOnce(&UiCx) -> NodeKey,
) -> Result<(), Box<dyn Error>> {
    let ui = UiRoot::new(root);
    let event_loop = EventLoop::<accesskit_winit::Event>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    Ok(event_loop.run_app(&mut App {
        attributes,
        proxy,
        adapter: None,
        ui,
        pipeline: Pipeline::default(),
        cursor: None,
        hovered: Vec::new(),
        start: Instant::now(),
        win: None,
        renderer: VelloGpuWindowRenderer::new(),
    })?)
}

struct App {
    attributes: WindowAttributes,
    /// Delivers accessibility events to the event loop.
    proxy: EventLoopProxy<accesskit_winit::Event>,
    /// Accessibility adapter of the window.
    adapter: Option<Adapter>,
    ui: UiRoot,
    pipeline: Pipeline,
    /// Cursor position in logical units, the units of the layout.
    cursor: Option<kurbo::Point>,
    /// Nodes with hover handlers under the cursor.
    hovered: Vec<NodeKey>,
    /// Origin of frame times.
    start: Instant,
    win: Option<Arc<Window>>,
    renderer: VelloGpuWindowRenderer,
}

impl ApplicationHandler<accesskit_winit::Event> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // The adapter must be created before the window is first shown.
        let visible = self.attributes.visible;
        let win: Arc<Window> = event_loop
            .create_window(self.attributes.clone().with_visible(false))
            .unwrap()
            .into();
        self.adapter = Some(Adapter::with_event_loop_proxy(
            event_loop,
            &win,
            self.proxy.clone(),
        ));
        win.set_visible(visible);
        let size = win.inner_size();

        if size.width > 0 && size.height > 0 {
            self.renderer
                .resume(win.clone(), size.width, size.height, || {});
            self.renderer.complete_resume();
        }

        win.request_redraw();
        self.win = Some(win);
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.renderer.suspend();
        self.adapter = None;
        self.pipeline.set_accessibility(false);
        self.win.take();
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: accesskit_winit::Event) {
        let Some(win) = self.win.clone() else {
            return;
        };

        if win.id() != event.window_id {
            return;
        }

        match event.window_event {
            // The next layout builds the whole tree, and sends it.
            AccessKitEvent::InitialTreeRequested => {
                self.pipeline.set_accessibility(true);
                win.request_redraw();
            }
            AccessKitEvent::ActionRequested(request) => {
                if request.action == Action::Click {
                    let path = {
                        let target = self.pipeline.accessibility_target(request.target_node);
                        ancestors(&self.ui.tree().borrow().ui, target)
                    };
                    run_click(&path);
                }
            }
            AccessKitEvent::AccessibilityDeactivated => self.pipeline.set_accessibility(false),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(win) = self.win.clone() else {
            return;
        };

        if win.id() != id {
            return;
        }

        if let Some(adapter) = &mut self.adapter {
            adapter.process_event(&win, &event);
        }

        match event {
            WindowEvent::Resized(size) => {
                if size.width == 0 || size.height == 0 {
                    return;
                }

                if self.renderer.is_active() {
                    self.renderer.set_size(size.width, size.height);
                } else {
                    self.renderer
                        .resume(win.clone(), size.width, size.height, || {});
                    self.renderer.complete_resume();
                }

                win.request_redraw();
            }
            WindowEvent::RedrawRequested => self.redraw(&win),
            WindowEvent::CursorMoved { position, .. } => {
                // Hit testing works in logical units, like the layout.
                let position = position.to_logical::<f64>(win.scale_factor());
                self.cursor = Some(kurbo::Point::new(position.x, position.y));
                self.update_hover();
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.update_hover();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.click(),
            // The logical viewport changes with the scale factor. A resize
            // usually follows, but the surface may keep its physical size.
            WindowEvent::ScaleFactorChanged { .. } => win.request_redraw(),
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }

    /// Run the bindings woken by this batch of events, then draw a frame if
    /// they changed the tree or something animates.
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        self.ui.poll();

        let changed = !self.ui.tree().borrow().changes().is_empty();
        if (changed || event::animating())
            && let Some(win) = &self.win
        {
            win.request_redraw();
        }
    }
}

impl App {
    fn redraw(&mut self, win: &Window) {
        // This frame shows every change so far. Changes are taken even
        // without a frame, so they do not ask for one again.
        self.pipeline.take_changes(&mut self.ui.tree().borrow_mut());
        let size = win.inner_size();

        if size.width == 0 || size.height == 0 || !self.renderer.is_active() {
            return;
        }

        // Frame handlers set signals: run the bindings they woke before laying
        // out, and keep drawing frames while anything animates.
        event::run_frame(self.start.elapsed());
        self.ui.poll();
        if event::animating() {
            win.request_redraw();
        }

        // Layout works in logical units: the physical surface divided by the
        // display's scale factor.
        let scale_factor = win.scale_factor();
        let physical = kurbo::Rect::new(0.0, 0.0, size.width as f64, size.height as f64);
        let viewport = Size::new(
            physical.width() / scale_factor,
            physical.height() / scale_factor,
        );

        let mut tree = self.ui.tree().borrow_mut();
        if self
            .pipeline
            .layout(&mut tree, self.ui.root(), viewport)
            .is_none()
        {
            return;
        }

        if let Some(update) = self.pipeline.take_accessibility_update(scale_factor)
            && let Some(adapter) = &mut self.adapter
        {
            adapter.update_if_active(|| update);
        }

        self.renderer.render(|scene| {
            self.pipeline.paint(&tree, physical, scale_factor, scene);
        });
    }

    /// The node under the cursor and its ancestors, innermost first, from the
    /// last frame's layout.
    fn path_under_cursor(&mut self) -> Vec<NodeKey> {
        let hit = self.cursor.and_then(|cursor| self.pipeline.hit(cursor));

        ancestors(&self.ui.tree().borrow().ui, hit)
    }

    /// Call the click handlers of the node under the cursor and its
    /// ancestors, innermost first.
    fn click(&mut self) {
        run_click(&self.path_under_cursor());
    }

    /// Call the hover handlers of the nodes the cursor left, then of those it
    /// entered.
    fn update_hover(&mut self) {
        let path = self.path_under_cursor();
        let hovered = event::hover_targets(&path);

        let left = self.hovered.iter().filter(|node| !hovered.contains(node));
        let entered = hovered.iter().filter(|node| !self.hovered.contains(node));
        let changes: Vec<_> = left
            .map(|&node| (node, false))
            .chain(entered.map(|&node| (node, true)))
            .collect();
        self.hovered = hovered;

        for (node, value) in changes {
            if let Some(handler) = event::hover_handler(node) {
                handler(value);
            }
        }
    }
}

fn ancestors(ui: &Ui, node: Option<NodeKey>) -> Vec<NodeKey> {
    core::iter::successors(node, |&node| ui.parent(node)).collect()
}

/// Call the click handlers of `path`, a node and its ancestors, innermost
/// first. Handlers run without borrows of the tree.
fn run_click(path: &[NodeKey]) {
    for handler in event::click_handlers(path) {
        handler();
    }
}
