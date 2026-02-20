use core::error::Error;
use std::sync::Arc;

use anyrender::WindowRenderer;
use anyrender_vello::VelloWindowRenderer;
use comet_div::ui::{NodeKey, Ui};
use comet_layout::{
    cx::LayoutContext,
    tree::{LayoutNodeKey, LayoutTree, builder::UiLayoutBuilder},
};
use comet_renderer::CometRenderer;
use parley::FontContext;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

pub fn run(ui: Ui, root: NodeKey) -> Result<(), Box<dyn Error>> {
    let ev = EventLoop::new()?;

    let mut layout_tree = LayoutTree::new();
    let layout_root = layout_tree.create_root();

    Ok(ev.run_app(&mut App {
        ui,
        ui_root: root,

        builder: UiLayoutBuilder::new(),

        font_cx: FontContext::new(),
        layout_cx: LayoutContext::new(),
        layout_tree,
        layout_root,

        win: None,
        vello: VelloWindowRenderer::new(),
        ui_renderer: CometRenderer::new(),
    })?)
}

// TODO:: proper impl
struct App {
    ui: Ui,
    ui_root: NodeKey,

    builder: UiLayoutBuilder,

    font_cx: FontContext,
    layout_cx: LayoutContext,
    layout_tree: LayoutTree,
    layout_root: LayoutNodeKey,

    win: Option<Arc<Window>>,
    vello: VelloWindowRenderer,
    ui_renderer: CometRenderer,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win = self.win.insert(
            event_loop
                .create_window(WindowAttributes::default())
                .unwrap()
                .into(),
        );

        // TODO:: reactive building
        self.builder.build(
            &self.ui,
            self.ui_root,
            &mut self.layout_tree,
            self.layout_root,
        );

        let surface_size = win.inner_size();
        self.vello
            .resume(win.clone(), surface_size.width, surface_size.height);
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.win.take();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(resized) => {
                // Ignore zero size surface
                if resized.width == 0 || resized.height == 0 {
                    return;
                }

                self.vello.set_size(resized.width, resized.height);
            }

            WindowEvent::RedrawRequested => {
                let surface_size = self.win.as_ref().unwrap().inner_size();
                if surface_size.width == 0 || surface_size.height == 0 {
                    return;
                }

                // Perform layout
                self.layout_cx.layout(
                    &mut self.font_cx,
                    &mut self.ui,
                    &mut self.layout_tree,
                    self.layout_root,
                    (surface_size.width as _, surface_size.height as _),
                );

                self.vello.render(|painter| {
                    self.ui_renderer
                        .draw(&self.ui, &self.layout_tree, self.layout_root, painter);
                });
            }

            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            _ => {}
        }
    }
}
