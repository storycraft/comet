use core::error::Error;
use std::sync::Arc;

use anyrender::WindowRenderer;
use anyrender_vello_cpu::VelloCpuWindowRenderer;
use comet_layout::{
    fragment::{builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{LayoutConstraint, geometry::Size},
};
use comet_paint::CometPainter;
use comet_tree::{node::NodeKey, ui::Ui};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

/// Font size of the root element, used for `rem`.
const ROOT_FONT_SIZE: f64 = 16.0;

pub fn run(ui: Ui, root: NodeKey) -> Result<(), Box<dyn Error>> {
    if ui.kind(root).is_none() {
        return Err("invalid UI root".into());
    }

    let event_loop = EventLoop::new()?;

    Ok(event_loop.run_app(&mut App {
        ui,
        ui_root: root,
        input_builder: LayoutInputTreeBuilder::new(),
        fragment_builder: FragmentTreeBuilder::new(),
        inputs: LayoutInputTree::new(),
        fragments: FragmentTree::new(),
        win: None,
        vello: VelloCpuWindowRenderer::new(),
        ui_renderer: CometPainter::new(),
    })?)
}

struct App {
    ui: Ui,
    ui_root: NodeKey,
    input_builder: LayoutInputTreeBuilder,
    fragment_builder: FragmentTreeBuilder,
    inputs: LayoutInputTree,
    fragments: FragmentTree,
    win: Option<Arc<Window>>,
    vello: VelloCpuWindowRenderer,
    ui_renderer: CometPainter,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win: Arc<Window> = event_loop
            .create_window(WindowAttributes::default())
            .unwrap()
            .into();
        let size = win.inner_size();

        if size.width > 0 && size.height > 0 {
            self.vello.resume(win.clone(), size.width, size.height, || {
                // TODO
            });
            self.vello.complete_resume();
        }

        win.request_redraw();
        self.win = Some(win);
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.vello.suspend();
        self.win.take();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(win) = &self.win else {
            return;
        };

        if win.id() != id {
            return;
        }

        match event {
            WindowEvent::Resized(size) => {
                if size.width == 0 || size.height == 0 {
                    return;
                }

                if self.vello.is_active() {
                    self.vello.set_size(size.width, size.height);
                } else {
                    self.vello.resume(win.clone(), size.width, size.height, || {
                        // TODO
                    });
                    self.vello.complete_resume();
                }

                win.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let size = win.inner_size();

                if size.width == 0 || size.height == 0 || !self.vello.is_active() {
                    return;
                }

                let viewport = Size::new(size.width as f64, size.height as f64);
                let Some(input_root) = self.input_builder.build(
                    &self.ui,
                    &mut self.inputs,
                    BuildRequest::new(self.ui_root, ROOT_FONT_SIZE, viewport),
                ) else {
                    return;
                };

                let constraint = LayoutConstraint::new_root(viewport, ROOT_FONT_SIZE);
                let fragment_root = self.fragment_builder.build(
                    &self.ui,
                    &self.inputs,
                    input_root,
                    &mut self.fragments,
                    &constraint,
                );
                // Flags were consumed by the fragment build.
                self.inputs.clear_flags(input_root);

                let Some(fragment_root) = fragment_root else {
                    return;
                };

                self.vello.render(|painter| {
                    self.ui_renderer.draw(
                        &self.ui,
                        &self.inputs,
                        &self.fragments,
                        fragment_root,
                        &constraint.units,
                        kurbo::Rect::new(0.0, 0.0, viewport.width, viewport.height),
                        painter,
                    );
                });
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }
}
