//! Minimal winit integration for the examples: sets up `reactive_graph`,
//! opens a window, lays out and paints the tree, and dispatches clicks and
//! hovers to handlers registered with [`on_click`] and [`on_hover`].
//!
//! Bindings re-run as local tasks on a `futures` `LocalPool`, polled after
//! every batch of window events. Signals here are only set from window
//! events, so that is enough; signals set from other threads would also need
//! to wake the event loop.

use core::error::Error;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use any_spawner::{CustomExecutor, Executor, PinnedFuture, PinnedLocalFuture};
use anyrender::WindowRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use comet_layout::{
    fragment::{builder::FragmentTreeBuilder, tree::FragmentTree},
    input::builder::{BuildRequest, LayoutInputTreeBuilder},
    style::{LayoutConstraint, geometry::Size},
};
use comet_paint::{
    hit::PaintHitTester,
    layer::{PaintLayerKey, builder::PaintLayerBuilder, tree::PaintLayerTree},
    painter::CometPainter,
    prepaint::{builder::PrepaintTreeBuilder, tree::PrepaintTree},
};
use comet_reactivity::{UiCx, UiTree};
use comet_tree::{node::NodeKey, ui::Ui};
use futures::{
    executor::{LocalPool, LocalSpawner},
    task::LocalSpawnExt,
};
use reactive_graph::owner::Owner;
use softbuffer_window_renderer::SoftbufferWindowRenderer;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

const ROOT_FONT_SIZE: f64 = 16.0;

thread_local! {
    static HANDLERS: RefCell<Handlers> = RefCell::default();
}

#[derive(Default)]
struct Handlers {
    click: HashMap<NodeKey, Rc<dyn Fn()>>,
    hover: HashMap<NodeKey, Rc<dyn Fn(bool)>>,
}

/// Call `handler` when `node` or one of its descendants is clicked.
pub fn on_click(node: NodeKey, handler: impl Fn() + 'static) {
    HANDLERS.with_borrow_mut(|handlers| handlers.click.insert(node, Rc::new(handler)));
}

/// Call `handler` when the cursor enters (`true`) or leaves (`false`) `node`
/// and its descendants.
pub fn on_hover(node: NodeKey, handler: impl Fn(bool) + 'static) {
    HANDLERS.with_borrow_mut(|handlers| handlers.hover.insert(node, Rc::new(handler)));
}

/// Build the UI with `root` and show it in a window until it is closed.
pub fn run(title: &str, root: impl FnOnce(&UiCx) -> NodeKey) -> Result<(), Box<dyn Error>> {
    Executor::init_local_custom_executor(LocalExecutor::new())?;
    let owner = Owner::new();
    owner.set();

    let tree = Rc::new(RefCell::new(UiTree::new()));
    let cx = UiCx::new(tree.clone());
    let root = cx.build(root);

    EventLoop::new()?.run_app(&mut App {
        title: title.to_owned(),
        tree,
        root,
        pipeline: Pipeline::default(),
        cursor: None,
        hovered: Vec::new(),
        win: None,
        renderer: SoftbufferWindowRenderer::new(),
        painter: CometPainter::new(),
        _cx: cx,
    })?;

    Ok(())
}

struct App {
    title: String,
    tree: Rc<RefCell<UiTree>>,
    root: NodeKey,
    pipeline: Pipeline,
    /// Cursor position in logical units.
    cursor: Option<kurbo::Point>,
    /// Nodes with hover handlers under the cursor.
    hovered: Vec<NodeKey>,
    win: Option<Arc<Window>>,
    renderer: SoftbufferWindowRenderer<VelloCpuImageRenderer>,
    painter: CometPainter,
    /// Owns the bindings; dropping it stops them.
    _cx: UiCx,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = WindowAttributes::default().with_title(&self.title);
        let win: Arc<Window> = event_loop.create_window(attributes).unwrap().into();
        let size = win.inner_size();
        self.renderer.resume(win.clone(), size.width, size.height, || {});
        self.renderer.complete_resume();
        win.request_redraw();
        self.win = Some(win);
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.renderer.suspend();
        self.win = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(win) = self.win.clone() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.renderer.set_size(size.width, size.height);
                    win.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => win.request_redraw(),
            WindowEvent::CursorMoved { position, .. } => {
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
            WindowEvent::RedrawRequested => self.redraw(&win),
            _ => {}
        }
    }

    /// Run the bindings woken by this batch of events, then draw a frame if
    /// they changed the tree.
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        Executor::poll_local();

        if self.tree.borrow_mut().take_redraw()
            && let Some(win) = &self.win
        {
            win.request_redraw();
        }
    }
}

impl App {
    fn redraw(&mut self, win: &Window) {
        let size = win.inner_size();
        if size.width == 0 || size.height == 0 || !self.renderer.is_active() {
            return;
        }

        let scale_factor = win.scale_factor();
        let physical = kurbo::Rect::new(0.0, 0.0, size.width as f64, size.height as f64);
        let viewport = Size::new(
            physical.width() / scale_factor,
            physical.height() / scale_factor,
        );

        let mut tree = self.tree.borrow_mut();
        let Some(layer_root) = self.pipeline.layout(&mut tree, self.root, viewport) else {
            return;
        };

        let Pipeline {
            fragments,
            prepaint,
            layers,
            ..
        } = &self.pipeline;
        self.renderer.render(|scene| {
            self.painter.draw(
                &tree.ui,
                &tree.inputs,
                fragments,
                prepaint,
                layers,
                layer_root,
                physical,
                scale_factor,
                scene,
            );
        });
    }

    /// The node under the cursor and its ancestors, innermost first.
    fn path_under_cursor(&mut self) -> Vec<NodeKey> {
        let tree = self.tree.borrow();
        let hit = self
            .cursor
            .and_then(|cursor| self.pipeline.hit(&tree, cursor));
        ancestors(&tree.ui, hit)
    }

    fn click(&mut self) {
        let path = self.path_under_cursor();
        // Handlers run without borrows, so they may register handlers.
        let handlers: Vec<_> = HANDLERS.with_borrow(|handlers| {
            path.iter()
                .filter_map(|node| handlers.click.get(node).cloned())
                .collect()
        });

        for handler in handlers {
            handler();
        }
    }

    fn update_hover(&mut self) {
        let path = self.path_under_cursor();
        let hovered: Vec<_> = HANDLERS.with_borrow(|handlers| {
            path.into_iter()
                .filter(|node| handlers.hover.contains_key(node))
                .collect()
        });

        let left = self.hovered.iter().filter(|node| !hovered.contains(node));
        let entered = hovered.iter().filter(|node| !self.hovered.contains(node));
        let changes: Vec<_> = left
            .map(|&node| (node, false))
            .chain(entered.map(|&node| (node, true)))
            .collect();
        self.hovered = hovered;

        for (node, value) in changes {
            if let Some(handler) = HANDLERS.with_borrow(|handlers| handlers.hover.get(&node).cloned()) {
                handler(value);
            }
        }
    }
}

fn ancestors(ui: &Ui, node: Option<NodeKey>) -> Vec<NodeKey> {
    core::iter::successors(node, |&node| ui.parent(node)).collect()
}

/// Layout and paint state of the window. Every step is incremental, so a
/// layout without marked changes or a new viewport does little work.
#[derive(Default)]
struct Pipeline {
    input_builder: LayoutInputTreeBuilder,
    fragment_builder: FragmentTreeBuilder,
    fragments: FragmentTree,
    prepaint_builder: PrepaintTreeBuilder,
    prepaint: PrepaintTree,
    layer_builder: PaintLayerBuilder,
    layers: PaintLayerTree,
    hit_tester: PaintHitTester,
    layer_root: Option<PaintLayerKey>,
    viewport: Option<Size<f64>>,
}

impl Pipeline {
    /// Lay out the changes marked in `tree` and place the layers, returning
    /// the root layer.
    fn layout(
        &mut self,
        tree: &mut UiTree,
        root: NodeKey,
        viewport: Size<f64>,
    ) -> Option<PaintLayerKey> {
        let UiTree {
            ui, inputs, dirty, ..
        } = tree;
        let viewport_changed = self.viewport.is_some_and(|previous| previous != viewport);
        self.layer_root = None;

        if viewport_changed && let Some(key) = inputs.item_for_node(root) {
            inputs.invalidate_viewport(dirty, key);
        }
        let input_root = self.input_builder.build(
            ui,
            inputs,
            dirty,
            BuildRequest::new(root, ROOT_FONT_SIZE, viewport),
        )?;

        let constraint = LayoutConstraint::new_root(viewport, ROOT_FONT_SIZE)
            .with_viewport_changed(viewport_changed);
        let fragment_root = self.fragment_builder.build(
            ui,
            inputs,
            dirty,
            input_root,
            &mut self.fragments,
            &constraint,
        )?;
        self.prepaint_builder.build(
            ui,
            inputs,
            dirty,
            &self.fragments,
            &constraint.units,
            &mut self.prepaint,
            fragment_root,
            viewport_changed,
        )?;
        let layer_root = self.layer_builder.build(
            &self.fragments,
            inputs,
            dirty,
            &self.prepaint,
            &mut self.layers,
            input_root,
        )?;

        dirty.clear(inputs, input_root);
        self.viewport = Some(viewport);
        self.layer_root = Some(layer_root);
        Some(layer_root)
    }

    /// The node under `point`, from the last layout.
    fn hit(&mut self, tree: &UiTree, point: kurbo::Point) -> Option<NodeKey> {
        let key = self.hit_tester.hit(
            &self.layers,
            &self.fragments,
            &self.prepaint,
            self.layer_root?,
            point,
        )?;
        let source = self.fragments.items.get(key)?.source;
        Some(tree.inputs.items.get(source)?.node)
    }
}

/// Runs local tasks on a `LocalPool` when polled.
struct LocalExecutor {
    pool: RefCell<LocalPool>,
    spawner: LocalSpawner,
}

impl LocalExecutor {
    fn new() -> Self {
        let pool = LocalPool::new();
        let spawner = pool.spawner();
        Self {
            pool: RefCell::new(pool),
            spawner,
        }
    }
}

impl CustomExecutor for LocalExecutor {
    // Everything runs on the event loop thread.
    fn spawn(&self, fut: PinnedFuture<()>) {
        self.spawn_local(fut);
    }

    fn spawn_local(&self, fut: PinnedLocalFuture<()>) {
        self.spawner.spawn_local(fut).expect("pool is alive");
    }

    fn poll_local(&self) {
        // A task polling the executor itself is ignored.
        if let Ok(mut pool) = self.pool.try_borrow_mut() {
            pool.run_until_stalled();
        }
    }
}
