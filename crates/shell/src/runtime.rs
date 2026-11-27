//! The reactive runtime: the `any_spawner` executor bindings re-run on, and
//! the shared tree a UI is built in.

use std::{cell::RefCell, rc::Rc};

use any_spawner::{CustomExecutor, Executor, PinnedFuture, PinnedLocalFuture};
use comet_reactivity::{cx::UiCx, tree::UiTree};
use comet_tree::node::NodeKey;
use futures::{
    executor::{LocalPool, LocalSpawner},
    task::LocalSpawnExt,
};

/// A UI built with [`comet_reactivity`], in its own shared [`UiTree`].
///
/// The node built by the root component is the root of the layout, laid
/// out against the viewport.
///
/// Its bindings stop once it is dropped.
pub struct UiRoot {
    tree: Rc<RefCell<UiTree>>,
    cx: UiCx,
    root: NodeKey,
}

impl UiRoot {
    /// Build the UI by running `component` once, under a new owner.
    ///
    /// Sets up the executor of the current thread first, unless one is set up
    /// already. Bindings re-run on it when [`poll`](Self::poll) is called,
    /// which [`Window::poll`](crate::window::Window::poll) does for you.
    pub fn new(component: impl FnOnce(&UiCx) -> NodeKey) -> Self {
        init_executor();

        let tree = Rc::new(RefCell::new(UiTree::new()));
        let cx = UiCx::new(tree.clone());
        let root = cx.build(component);

        Self { tree, cx, root }
    }

    pub fn tree(&self) -> &Rc<RefCell<UiTree>> {
        &self.tree
    }

    pub fn cx(&self) -> &UiCx {
        &self.cx
    }

    /// The node built by the root component.
    pub fn root(&self) -> NodeKey {
        self.root
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
