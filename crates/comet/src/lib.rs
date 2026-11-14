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
use comet_paint::{
    hit::PaintHitTester,
    layer::{PaintLayerKey, builder::PaintLayerBuilder, tree::PaintLayerTree},
    painter::CometPainter,
    pre_paint::{resolver::PrePaintTreeResolver, tree::PrePaintTree},
};
use comet_tree::{node::NodeKey, ui::Ui};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
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
        pre_paint: PrePaintTree::new(),
        pre_paint_resolver: PrePaintTreeResolver::new(),
        layers: PaintLayerTree::new(),
        layer_builder: PaintLayerBuilder::new(),
        layer_root: None,
        hit_tester: PaintHitTester::new(),
        cursor: None,
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
    pre_paint: PrePaintTree,
    pre_paint_resolver: PrePaintTreeResolver,
    layers: PaintLayerTree,
    layer_builder: PaintLayerBuilder,
    /// Root layer of `layers`, from the last frame.
    layer_root: Option<PaintLayerKey>,
    // TODO: Temporary hit testing, printing the fragment under the cursor on
    // click. Replace with real input handling.
    hit_tester: PaintHitTester,
    /// Cursor position in physical pixels, the units of the layout viewport.
    cursor: Option<kurbo::Point>,
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

                // Positions and clips follow the layout, which was just
                // built, so resolve the whole tree again.
                if self
                    .pre_paint_resolver
                    .resolve(
                        &self.ui,
                        &self.inputs,
                        &self.fragments,
                        &constraint.units,
                        &mut self.pre_paint,
                        fragment_root,
                    )
                    .is_none()
                {
                    return;
                }

                self.layer_root = self.layer_builder.build(
                    &self.ui,
                    &self.inputs,
                    &self.fragments,
                    &mut self.layers,
                    fragment_root,
                );
                let Some(layer_root) = self.layer_root else {
                    return;
                };

                self.vello.render(|painter| {
                    self.ui_renderer.draw(
                        &self.ui,
                        &self.inputs,
                        &self.fragments,
                        &self.pre_paint,
                        &self.layers,
                        layer_root,
                        kurbo::Rect::new(0.0, 0.0, viewport.width, viewport.height),
                        painter,
                    );
                });
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some(kurbo::Point::new(position.x, position.y));
            }
            WindowEvent::CursorLeft { .. } => self.cursor = None,
            // TODO: Temporary hit testing, printing the fragment under the
            // cursor to stdout. Replace with real input handling.
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.print_hit(),
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }
}

impl App {
    /// Print the fragment under the cursor, from the last frame's layout.
    ///
    /// TODO: Temporary, for checking hit testing by hand. Remove once input
    /// events are dispatched to the UI.
    fn print_hit(&mut self) {
        let (Some(cursor), Some(layer_root)) = (self.cursor, self.layer_root) else {
            return;
        };

        let Some(key) = self.hit_tester.hit(
            &self.layers,
            &self.fragments,
            &self.pre_paint,
            layer_root,
            cursor,
        ) else {
            println!("hit ({}, {}): nothing", cursor.x, cursor.y);
            return;
        };

        let Some(fragment) = self.fragments.items.get(key) else {
            return;
        };
        let node = self.inputs.items.get(fragment.source).map(|item| item.node);

        println!(
            "hit ({}, {}): {key:?} {:?}, node {node:?}",
            cursor.x, cursor.y, fragment.item
        );
    }
}
