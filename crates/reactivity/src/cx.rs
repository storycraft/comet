//! Building reactive nodes.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use comet_tree::{
    builder::{NodeBuilder, ReservedNodeKey},
    node::{Node, NodeKey, Text},
    style::StyleProps,
};
use reactive_graph::{
    effect::RenderEffect,
    owner::{Owner, StoredValue},
};

use crate::{
    IntoView,
    prop::{PropHint, ReactiveProp},
    tree::UiTree,
};

/// Context for building nodes in a shared [`UiTree`].
///
/// Nodes built with it are not attached to any parent: components return
/// them as a [`IntoView`], and [`NodeBuild::children`] appends the view returned
/// by its closure to the node being built.
///
/// Signals and bindings created through it belong to its [`Owner`], and stop
/// once the last handle to that owner is dropped, so keep the root `UiCx`
/// alive while the UI is in use. The `reactive_graph` setup, such as the
/// executor that re-runs bindings, is left to the application.
#[derive(Clone)]
pub struct UiCx {
    tree: Rc<RefCell<UiTree>>,
    owner: Owner,
}

impl UiCx {
    /// A context with a new [`Owner`], a child of the current one if any.
    pub fn new(tree: Rc<RefCell<UiTree>>) -> Self {
        Self::with_owner(tree, Owner::new())
    }

    pub fn with_owner(tree: Rc<RefCell<UiTree>>, owner: Owner) -> Self {
        Self { tree, owner }
    }

    pub fn tree(&self) -> &Rc<RefCell<UiTree>> {
        &self.tree
    }

    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    /// A context building in the same tree under a new child [`Owner`] of
    /// this context's owner. The child owner lives as long as this owner,
    /// and is cleaned up with it.
    pub fn child(&self) -> Self {
        let owner = self.owner.child();
        // Owners hold their children weakly: the parent's arena keeps it.
        self.owner.with(|| StoredValue::new(owner.clone()));

        Self::with_owner(self.tree.clone(), owner)
    }

    /// Run a component under this context's owner.
    pub fn build<V: IntoView>(&self, component: impl FnOnce(&UiCx) -> V) -> V {
        self.owner.with(|| component(self))
    }

    /// Start building a box node.
    pub fn div(&self) -> NodeBuild<'_> {
        NodeBuild::new(self, Node::Box)
    }

    /// Start building a text node with fixed content.
    pub fn text(&self, text: impl Into<String>) -> NodeBuild<'_> {
        let mut build = NodeBuild::new(self, Node::Text);
        build.builder.add(Text(text.into()));
        build
    }

    /// Start building a text node whose content follows `text`.
    pub fn text_with(&self, text: impl Fn() -> String + 'static) -> NodeBuild<'_> {
        NodeBuild::new(self, Node::Text).style_with(move || Text(text()))
    }
}

/// A node being built. Created with [`UiCx::div`] or [`UiCx::text`] and
/// finished with [`Self::build`], or with [`Self::children`] to build its
/// children as well.
#[must_use = "the node is only created by `build` or `children`"]
pub struct NodeBuild<'a> {
    cx: &'a UiCx,
    kind: Node,
    /// Taken when the node is created; released on drop otherwise.
    key: Option<ReservedNodeKey>,
    builder: NodeBuilder,
}

impl<'a> NodeBuild<'a> {
    fn new(cx: &'a UiCx, kind: Node) -> Self {
        Self {
            key: Some(cx.tree.borrow().ui.reserve_node()),
            cx,
            kind,
            builder: NodeBuilder::new(),
        }
    }

    /// The context the node is built with.
    pub fn cx(&self) -> &'a UiCx {
        self.cx
    }

    /// The key the node will have once built.
    pub fn key(&self) -> NodeKey {
        self.key.as_ref().expect("node is created once").key()
    }

    /// Add props, replacing earlier ones of the same types.
    pub fn style(mut self, props: impl StyleProps) -> Self {
        self.builder.add_bundle(props);
        self
    }

    /// Bind a prop to `f`. It runs now for the initial value, then again
    /// whenever a signal it read changes, writing the prop into the tree and
    /// recording its [`ReactiveProp::DIRTY`] flags.
    pub fn style_with<P: ReactiveProp>(mut self, f: impl Fn() -> P + 'static) -> Self {
        let node = self.key();
        let tree = Rc::downgrade(&self.cx.tree);

        // The synchronous first run returns its value, kept by the effect
        // until taken below. Later runs get `Some(None)` back as their
        // previous value, write into the tree and return `None`.
        let effect = self.cx.owner.with(|| {
            RenderEffect::new(move |prev: Option<Option<P>>| {
                let value = f();
                if prev.is_none() {
                    return Some(value);
                }

                write(&tree, node, value);
                None
            })
        });

        if let Some(value) = effect.with_value_mut(Option::take).flatten() {
            self.builder.add(value);
        }
        // Kept until the owner is cleaned up.
        self.cx.owner.with(|| StoredValue::new(effect));
        self
    }

    /// Create the node, without a parent. Returns the node's key, a
    /// [`IntoView`] of the node.
    pub fn build(mut self) -> NodeKey {
        self.create()
    }

    /// Create the node like [`Self::build`], then build its children with
    /// `children` and append the nodes of the view it returns to the node,
    /// in order. Nodes attached elsewhere are moved. Returns the node's key.
    ///
    /// `children` runs with a [child context](UiCx::child), so the signals
    /// and bindings of the children belong to a child [`Owner`]. The view is
    /// consumed under that owner too, so it may build nodes lazily.
    pub fn children<V: IntoView>(mut self, children: impl FnOnce(&UiCx) -> V) -> NodeKey {
        let key = self.create();
        let cx = self.cx.child();

        let attached = cx.owner.with(|| {
            let mut attached = false;
            for node in children(&cx) {
                // Borrowed for each node only, as the view may build the
                // next one while iterated.
                let mut tree = cx.tree.borrow_mut();
                if let Some(old) = tree.ui.append(key, node) {
                    tree.mark(old, PropHint::CHILDREN);
                }
                attached = true;
            }
            attached
        });

        if attached {
            // New children are built with the parent's children.
            cx.tree.borrow_mut().mark(key, PropHint::CHILDREN);
        }

        key
    }

    fn create(&mut self) -> NodeKey {
        let reserved = self.key.take().expect("node is created once");

        self.cx
            .tree
            .borrow_mut()
            .ui
            .create_prebuilt(reserved, self.builder.build(self.kind))
            .expect("reserved key is created once")
    }
}

impl Drop for NodeBuild<'_> {
    fn drop(&mut self) {
        // Not built: bindings stay with the owner but write nothing to a missing node.
        if let Some(key) = self.key.take()
            && let Ok(mut tree) = self.cx.tree.try_borrow_mut()
        {
            tree.ui.delete(key.key());
        }
    }
}

fn write<P: ReactiveProp>(tree: &Weak<RefCell<UiTree>>, node: NodeKey, value: P) {
    if let Some(tree) = tree.upgrade() {
        tree.borrow_mut().set_prop(node, value);
    }
}
