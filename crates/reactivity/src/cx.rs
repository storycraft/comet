//! Building reactive nodes.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use comet_layout::dirty_map::DirtyFlags;
use comet_tree::{
    builder::{NodeBuilder, ReservedNodeKey},
    node::{Node, NodeKey, Text},
    style::StyleProps,
};
use reactive_graph::{
    effect::RenderEffect,
    owner::{Owner, on_cleanup},
};

use crate::{prop::ReactiveProp, tree::UiTree};

/// Context for building nodes in a shared [`UiTree`].
///
/// Signals and bindings created through it belong to its [`Owner`], and stop
/// once the last handle to that owner is dropped, so keep the root `UiCx`
/// alive while the UI is in use. The `reactive_graph` setup, such as the
/// executor that re-runs bindings, is left to the application.
#[derive(Clone)]
pub struct UiCx {
    tree: Rc<RefCell<UiTree>>,
    owner: Owner,
    parent: Option<NodeKey>,
}

impl UiCx {
    /// A context with a new [`Owner`], a child of the current one if any.
    pub fn new(tree: Rc<RefCell<UiTree>>) -> Self {
        Self::with_owner(tree, Owner::new())
    }

    pub fn with_owner(tree: Rc<RefCell<UiTree>>, owner: Owner) -> Self {
        Self {
            tree,
            owner,
            parent: None,
        }
    }

    pub fn tree(&self) -> &Rc<RefCell<UiTree>> {
        &self.tree
    }

    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    /// Node that nodes built with this context are appended to.
    pub fn parent(&self) -> Option<NodeKey> {
        self.parent
    }

    /// The same context, appending to `parent` instead.
    pub fn with_parent(&self, parent: Option<NodeKey>) -> Self {
        Self {
            tree: self.tree.clone(),
            owner: self.owner.clone(),
            parent,
        }
    }

    /// Run a component under this context's owner.
    pub fn build<R>(&self, component: impl FnOnce(&UiCx) -> R) -> R {
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
    /// marking its [`ReactiveProp::DIRTY`] flags.
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
        self.cx.owner.with(|| on_cleanup(move || drop(effect)));
        self
    }

    /// Create the node and append it to the context's parent. Returns the
    /// node's key.
    pub fn build(mut self) -> NodeKey {
        self.create()
    }

    /// Create the node like [`Self::build`], then build its children with
    /// `children`, whose context appends to the node. Returns the node's key.
    pub fn children<R>(mut self, children: impl FnOnce(&UiCx) -> R) -> NodeKey {
        let key = self.create();
        self.cx.with_parent(Some(key)).build(children);
        key
    }

    fn create(&mut self) -> NodeKey {
        let reserved = self.key.take().expect("node is created once");
        let mut tree = self.cx.tree.borrow_mut();
        let UiTree {
            ui, inputs, dirty, ..
        } = &mut *tree;

        let key = ui
            .create_prebuilt(reserved, self.builder.build(self.kind))
            .expect("reserved key is created once");

        if let Some(parent) = self.cx.parent {
            ui.append(parent, key);
            // New children are built with the parent's children. Nothing is
            // marked before the parent is laid out.
            dirty.mark_node(inputs, parent, DirtyFlags::NEED_CHILDREN);
        }

        key
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
