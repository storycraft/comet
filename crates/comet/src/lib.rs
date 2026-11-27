//! Reactive UI in a winit window.
//!
//! Build the UI with [`comet_reactivity`] components and show it with
//! [`run`]. The window itself is a [`comet_shell::window::Window`]: this crate feeds
//! it winit events, translated with `ui-events-winit`, and draws its frames
//! with vello.
//!
//! While an assistive technology is active, the window exposes an
//! accessibility tree mirroring the layout through `accesskit_winit`.

mod cursor;
pub mod prelude;

use core::error::Error;
use std::sync::Arc;

use accesskit_winit::{Adapter, WindowEvent as AccessKitEvent};
use anyrender::WindowRenderer;
use anyrender_vello_gpu::VelloGpuWindowRenderer;
use comet_reactivity::cx::UiCx;
use comet_shell::window::Window;
use comet_tree::node::NodeKey;
use reactive_graph::{effect::RenderEffect, traits::Get};
use ui_events::{keyboard::KeyboardEvent, pointer::PointerEvent};
use ui_events_winit::{WindowEventReducer, WindowEventTranslation};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::{Window as WinitWindow, WindowAttributes, WindowId},
};

pub use accesskit;
pub use comet_reactivity as reactivity;
pub use comet_reactivity::view::IntoView;
pub use comet_shell as shell;
pub use comet_shell::{
    listeners::{on_frame, on_frame_while, on_next_frame, on_window_key, on_window_pointer},
    runtime::UiRoot,
    window::state::use_window,
};
pub use reactive_graph;
pub use ui_events;
pub use winit;

/// Build the UI with the `root` component and show it in a window opened with
/// `attributes`, until the window is closed.
pub fn run(
    attributes: WindowAttributes,
    root: impl FnOnce(&UiCx) -> NodeKey,
) -> Result<(), Box<dyn Error>> {
    let window = Window::new(root);
    let event_loop = EventLoop::<accesskit_winit::Event>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    Ok(event_loop.run_app(&mut App {
        attributes,
        proxy,
        adapter: None,
        window,
        reducer: WindowEventReducer::default(),
        win: None,
        cursor: None,
        renderer: VelloGpuWindowRenderer::new(),
    })?)
}

struct App {
    attributes: WindowAttributes,
    /// Delivers accessibility events to the event loop.
    proxy: EventLoopProxy<accesskit_winit::Event>,
    /// Accessibility adapter of the window.
    adapter: Option<Adapter>,
    window: Window,
    /// Translates winit input events into `ui-events`.
    reducer: WindowEventReducer,
    win: Option<Arc<WinitWindow>>,
    /// Shows the window's cursor signal over `win`, while it exists.
    cursor: Option<RenderEffect<()>>,
    renderer: VelloGpuWindowRenderer,
}

impl ApplicationHandler<accesskit_winit::Event> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // The adapter must be created before the window is first shown.
        let visible = self.attributes.visible;
        let win: Arc<WinitWindow> = event_loop
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
        self.window.resize(size, win.scale_factor());

        if size.width > 0 && size.height > 0 {
            self.renderer
                .resume(win.clone(), size.width, size.height, || {});
            self.renderer.complete_resume();
        }

        // Bindings re-run when the window is polled, after each batch of
        // events.
        self.cursor = Some(RenderEffect::new({
            let (win, cursor) = (win.clone(), self.window.state().cursor);
            move |_| cursor::set_cursor(&win, cursor.get())
        }));

        win.request_redraw();
        self.win = Some(win);
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.renderer.suspend();
        self.adapter = None;
        self.window.set_accessibility(false);
        self.cursor = None;
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
                self.window.set_accessibility(true);
                win.request_redraw();
            }
            AccessKitEvent::ActionRequested(request) => self.window.accessibility_action(&request),
            AccessKitEvent::AccessibilityDeactivated => self.window.set_accessibility(false),
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

        match &event {
            WindowEvent::Resized(size) => {
                self.window.resize(*size, win.scale_factor());

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
            }
            // A resize usually follows, but the surface may keep its
            // physical size.
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.window.resize(win.inner_size(), *scale_factor);
            }
            WindowEvent::RedrawRequested => self.redraw(&win),
            WindowEvent::Focused(focused) => self.window.set_focused(*focused),
            WindowEvent::Occluded(occluded) => self.window.set_occluded(*occluded),
            WindowEvent::CloseRequested => {
                if self.window.request_close() {
                    event_loop.exit();
                }
            }
            _ => match self.reducer.reduce(win.scale_factor(), &event) {
                Some(WindowEventTranslation::Pointer(event)) => self.pointer_event(&event),
                Some(WindowEventTranslation::Keyboard(event)) => self.keyboard_event(&event),
                None => {}
            },
        }
    }

    /// Run the bindings woken by this batch of events, then draw a frame if
    /// the window needs one.
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        self.window.poll();

        if self.window.needs_redraw()
            && let Some(win) = &self.win
        {
            win.request_redraw();
        }
    }
}

impl App {
    fn pointer_event(&mut self, event: &PointerEvent) {
        self.window.pointer_event(event);
    }

    fn keyboard_event(&mut self, event: &KeyboardEvent) {
        self.window.keyboard_event(event);
    }

    fn redraw(&mut self, win: &WinitWindow) {
        let laid_out = self.window.layout();

        // Keep drawing frames while anything animates.
        if self.window.needs_redraw() {
            win.request_redraw();
        }

        if !laid_out || !self.renderer.is_active() {
            return;
        }

        if let Some(update) = self.window.take_accessibility_update()
            && let Some(adapter) = &mut self.adapter
        {
            adapter.update_if_active(|| update);
        }

        let window = &mut self.window;
        self.renderer.render(|scene| window.paint(scene));
    }
}
