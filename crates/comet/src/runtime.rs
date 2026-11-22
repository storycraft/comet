//! The reactive runtime: the `any_spawner` executor bindings re-run on, and
//! the shared tree a UI is built in.

use std::{cell::RefCell, rc::Rc};

use any_spawner::{CustomExecutor, Executor, PinnedFuture, PinnedLocalFuture};
use comet_layout::style::{geometry::Point, taffy::Overflow, unit::*};
use comet_reactivity::{Styled, UiCx, UiTree};
use comet_tree::node::NodeKey;
use futures::{
    executor::{LocalPool, LocalSpawner},
    task::LocalSpawnExt,
};

/// A UI built with [`comet_reactivity`], in its own shared [`UiTree`].
///
/// The component is built inside a root scroll container filling the
/// viewport, which scrolls the UI when it overflows.
///
/// Its bindings stop once it is dropped.
pub struct UiRoot {
    tree: Rc<RefCell<UiTree>>,
    cx: UiCx,
    root: NodeKey,
    content: NodeKey,
}

impl UiRoot {
    /// Build the UI by running `component` once, under a new owner.
    ///
    /// Sets up the executor of the current thread first, unless one is set up
    /// already. Bindings re-run on it when [`poll`](Self::poll) is called,
    /// which [`run`](crate::run) does for you.
    pub fn new(component: impl FnOnce(&UiCx) -> NodeKey) -> Self {
        init_executor();

        let tree = Rc::new(RefCell::new(UiTree::new()));
        let cx = UiCx::new(tree.clone());
        let mut content = None;
        let root = cx.build(|cx| {
            cx.div()
                .size(size(vw(100), vh(100)))
                .overflow(Point::new(Overflow::Scroll, Overflow::Scroll))
                .children(|cx| content = Some(component(cx)))
        });
        let content = content.expect("component is built with the root");

        Self {
            tree,
            cx,
            root,
            content,
        }
    }

    pub fn tree(&self) -> &Rc<RefCell<UiTree>> {
        &self.tree
    }

    pub fn cx(&self) -> &UiCx {
        &self.cx
    }

    /// The root scroll container, filling the viewport and holding the
    /// component's node.
    pub fn root(&self) -> NodeKey {
        self.root
    }

    /// The node returned by the component.
    pub fn content(&self) -> NodeKey {
        self.content
    }

    /// Re-run the bindings whose signals changed. Do not hold a borrow of
    /// the tree, as bindings write into it.
    pub fn poll(&self) {
        Executor::poll_local();
    }
}

/// Set up the executor of the current thread, which runs tasks only when
/// polled with [`Executor::poll_local`]. Does nothing if the thread has one
/// already, such as one set up by the application.
pub fn init_executor() {
    let _ = Executor::init_local_custom_executor(LocalExecutor::new());
}

/// Runs tasks on a `LocalPool` when polled. Everything runs on the thread
/// that polls it.
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
    fn spawn(&self, fut: PinnedFuture<()>) {
        self.spawn_local(fut);
    }

    fn spawn_local(&self, fut: PinnedLocalFuture<()>) {
        self.spawner.spawn_local(fut).expect("pool is alive");
    }

    fn poll_local(&self) {
        // Polling from a task of the pool is ignored.
        if let Ok(mut pool) = self.pool.try_borrow_mut() {
            pool.run_until_stalled();
        }
    }
}
