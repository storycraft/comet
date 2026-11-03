use core::error::Error;
use std::sync::Arc;

use anyrender::WindowRenderer;
use anyrender_vello::VelloWindowRenderer;
use comet_layout::{
    fragment::{builder::FragmentBuilder, tree::FragmentTree},
    style::{LayoutConstraint, geometry::Size},
    tree::{LayoutItemKey, LayoutTree, builder::LayoutTreeBuilder},
};
use comet_renderer::CometRenderer;
use comet_tree::{node::NodeKey, ui::Ui};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

pub fn run(ui: Ui, root: NodeKey) -> Result<(), Box<dyn Error>> {
    let mut layout_tree = LayoutTree::new();
    let mut layout_builder = LayoutTreeBuilder::new();
    let layout_root = layout_builder
        .build(&ui, root, &mut layout_tree, None)
        .ok_or("invalid UI root")?;
    let event_loop = EventLoop::new()?;

    Ok(event_loop.run_app(&mut App {
        ui,
        ui_root: root,
        layout_builder,
        fragment_builder: FragmentBuilder::new(),
        layout_tree,
        layout_root,
        fragments: FragmentTree::new(),
        win: None,
        vello: VelloWindowRenderer::new(),
        ui_renderer: CometRenderer::new(),
    })?)
}

struct App {
    ui: Ui,
    ui_root: NodeKey,
    layout_builder: LayoutTreeBuilder,
    fragment_builder: FragmentBuilder,
    layout_tree: LayoutTree,
    layout_root: LayoutItemKey,
    fragments: FragmentTree,
    win: Option<Arc<Window>>,
    vello: VelloWindowRenderer,
    ui_renderer: CometRenderer,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win: Arc<Window> = event_loop
            .create_window(WindowAttributes::default())
            .unwrap()
            .into();
        let size = win.inner_size();

        if size.width > 0 && size.height > 0 {
            self.vello.resume(win.clone(), size.width, size.height);
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
                    self.vello.resume(win.clone(), size.width, size.height);
                }

                win.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let size = win.inner_size();

                if size.width == 0 || size.height == 0 || !self.vello.is_active() {
                    return;
                }

                let Some(root) = self.layout_builder.build(
                    &self.ui,
                    self.ui_root,
                    &mut self.layout_tree,
                    Some(self.layout_root),
                ) else {
                    return;
                };
                self.layout_root = root;
                let constraint = LayoutConstraint::new_root(
                    Size::new(size.width as f64, size.height as f64),
                    16.0,
                );
                let Some(root) = self.fragment_builder.build(
                    &constraint,
                    &self.ui,
                    &self.layout_tree,
                    root,
                    &mut self.fragments,
                ) else {
                    return;
                };

                self.vello.render(|painter| {
                    self.ui_renderer
                        .draw(&self.ui, &self.fragments, root, painter);
                });
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }
}
