use core::error::Error;
use std::sync::Arc;

use anyrender::Paint;
use anyrender::WindowRenderer;
use anyrender_vello::VelloWindowRenderer;
use comet_layout::style::geometry::StyleUnit;
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
    style::{OutlineFill, OutlineOffset, OutlineWidth},
};
use comet_tree::{node::NodeKey, ui::Ui};
use peniko::Color;
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
        pre_paint: PrePaintTree::new(),
        pre_paint_resolver: PrePaintTreeResolver::new(),
        layers: PaintLayerTree::new(),
        layer_builder: PaintLayerBuilder::new(),
        layer_root: None,
        viewport: None,
        hit_tester: PaintHitTester::new(),
        cursor: None,
        hovered: None,
        win: None,
        vello: VelloWindowRenderer::new(),
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
    /// Logical viewport of the last input build.
    viewport: Option<Size<f64>>,
    // TODO: Temporary hover outline, drawn around the node under the cursor.
    // Replace with real input handling.
    hit_tester: PaintHitTester,
    /// Cursor position in logical units, the units of the layout.
    cursor: Option<kurbo::Point>,
    /// The node outlined under the cursor, with the outline styles it had
    /// before, to restore once it is no longer hovered.
    hovered: Option<Hovered>,
    win: Option<Arc<Window>>,
    vello: VelloWindowRenderer,
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

                // Layout works in logical units: the physical surface divided
                // by the display's scale factor.
                let scale_factor = win.scale_factor();
                let physical = kurbo::Rect::new(0.0, 0.0, size.width as f64, size.height as f64);
                let viewport = Size::new(
                    physical.width() / scale_factor,
                    physical.height() / scale_factor,
                );
                // Boxes may depend on the viewport, so a resize or a scale
                // factor change lays out every box again.
                let forced = self.viewport.replace(viewport) != Some(viewport);
                let Some(input_root) = self.input_builder.build(
                    &self.ui,
                    &mut self.inputs,
                    BuildRequest::new(self.ui_root, ROOT_FONT_SIZE, viewport).forced(forced),
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
                        true,
                    )
                    .is_none()
                {
                    return;
                }

                // Layers keep their keys, so building from the root again
                // only places the layers whose owners changed.
                self.layer_root = self.layer_builder.build(
                    &self.fragments,
                    &self.pre_paint,
                    &mut self.layers,
                    fragment_root,
                    self.layer_root,
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
                        physical,
                        scale_factor,
                        painter,
                    );
                });
            }
            // TODO: Temporary hover outline. Replace with real input
            // handling.
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
            // The logical viewport changes with the scale factor. A resize
            // usually follows, but the surface may keep its physical size.
            WindowEvent::ScaleFactorChanged { .. } => win.request_redraw(),
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }
}

/// Outline drawn around the hovered node.
const HOVER_OUTLINE_WIDTH: f64 = 2.0;
const HOVER_OUTLINE_OFFSET: f64 = 2.0;

/// A node outlined because it is hovered, with the outline styles it set
/// itself.
struct Hovered {
    node: NodeKey,
    width: Option<OutlineWidth>,
    offset: Option<OutlineOffset>,
    fill: Option<OutlineFill>,
}

impl App {
    /// Outline the node under the cursor, from the last frame's layout, and
    /// restore the outline of the node hovered before.
    ///
    /// TODO: Temporary, for checking hit testing by hand. Remove once input
    /// events are dispatched to the UI.
    fn update_hover(&mut self) {
        let node = self.hovered_node();

        if self.hovered.as_ref().map(|hovered| hovered.node) == node {
            return;
        }

        if let Some(hovered) = self.hovered.take() {
            restore(&mut self.ui, hovered);
        }

        if let Some(node) = node {
            self.hovered = Some(outline(&mut self.ui, node));
        }

        if let Some(win) = &self.win {
            win.request_redraw();
        }
    }

    /// The source node of the fragment under the cursor.
    fn hovered_node(&mut self) -> Option<NodeKey> {
        let key = self.hit_tester.hit(
            &self.layers,
            &self.fragments,
            &self.pre_paint,
            self.layer_root?,
            self.cursor?,
        )?;
        let source = self.fragments.items.get(key)?.source;

        Some(self.inputs.items.get(source)?.node)
    }
}

/// Outline a hovered node, keeping the outline styles it set itself.
fn outline(ui: &mut Ui, node: NodeKey) -> Hovered {
    let props = ui.props(node);
    let hovered = Hovered {
        node,
        width: props.and_then(|props| props.get_cloned()),
        offset: props.and_then(|props| props.get_cloned()),
        fill: props.and_then(|props| props.get_cloned()),
    };

    ui.set_props(
        node,
        (
            OutlineWidth(StyleUnit::Px(HOVER_OUTLINE_WIDTH)),
            OutlineOffset(StyleUnit::Px(HOVER_OUTLINE_OFFSET)),
            OutlineFill(Paint::Solid(Color::BLACK)),
        ),
    );

    hovered
}

/// Restore the outline styles a node set itself, once no longer hovered.
fn restore(ui: &mut Ui, hovered: Hovered) {
    let Hovered {
        node,
        width,
        offset,
        fill,
    } = hovered;

    match width {
        Some(width) => ui.set_props(node, (width,)),
        None => drop(ui.remove_prop::<OutlineWidth>(node)),
    }

    match offset {
        Some(offset) => ui.set_props(node, (offset,)),
        None => drop(ui.remove_prop::<OutlineOffset>(node)),
    }

    match fill {
        Some(fill) => ui.set_props(node, (fill,)),
        None => drop(ui.remove_prop::<OutlineFill>(node)),
    }
}
