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

type ChildFn<'a> = Box<dyn FnOnce(&UiCx) + 'a>;

/// A node being built. Created with [`UiCx::div`] or [`UiCx::text`] and
/// finished with [`Self::build`].
#[must_use = "the node is only created by `build`"]
pub struct NodeBuild<'a> {
    cx: &'a UiCx,
    kind: Node,
    /// Taken by `build`; released on drop otherwise.
    key: Option<ReservedNodeKey>,
    builder: NodeBuilder,
    children: Vec<ChildFn<'a>>,
}

impl<'a> NodeBuild<'a> {
    fn new(cx: &'a UiCx, kind: Node) -> Self {
        Self {
            key: Some(cx.tree.borrow().ui.reserve_node()),
            cx,
            kind,
            builder: NodeBuilder::new(),
            children: Vec::new(),
        }
    }

    /// The key the node will have once built.
    pub fn key(&self) -> NodeKey {
        self.key.as_ref().expect("key taken by build").key()
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
        // Receives the value of the synchronous first run.
        let initial = Rc::new(RefCell::new(None::<P>));

        let effect = self.cx.owner.with(|| {
            let initial = initial.clone();
            RenderEffect::new(move |ran: Option<()>| {
                let value = f();
                match ran {
                    None => *initial.borrow_mut() = Some(value),
                    Some(()) => write(&tree, node, value),
                }
            })
        });
        self.cx.owner.with(|| on_cleanup(move || drop(effect)));

        if let Some(value) = initial.borrow_mut().take() {
            self.builder.add(value);
        }
        self
    }

    /// Run `child` once the node exists, with the node as its context's
    /// parent. Children are run in the order they were added.
    pub fn child<R>(mut self, child: impl FnOnce(&UiCx) -> R + 'a) -> Self {
        self.children.push(Box::new(move |cx| {
            child(cx);
        }));
        self
    }

    /// Create the node, append it to the context's parent, then run its
    /// children. Returns the node's key.
    pub fn build(mut self) -> NodeKey {
        let reserved = self.key.take().expect("key taken by build");

        let key = {
            let mut tree = self.cx.tree.borrow_mut();
            let UiTree {
                ui, inputs, dirty, ..
            } = &mut *tree;

            let key = ui
                .create_prebuilt(reserved, self.builder.build(self.kind))
                .expect("reserved key is created once");

            if let Some(parent) = self.cx.parent {
                ui.append(parent, key);
                // New children are built with the parent's children. Nothing
                // is marked before the parent is laid out.
                dirty.mark_node(inputs, parent, DirtyFlags::NEED_CHILDREN);
            }

            key
        };

        let children = core::mem::take(&mut self.children);
        if !children.is_empty() {
            let cx = self.cx.with_parent(Some(key));
            cx.build(|cx| {
                for child in children {
                    child(cx);
                }
            });
        }

        key
    }
}

impl Drop for NodeBuild<'_> {
    fn drop(&mut self) {
        // Bindings stay with the owner but write nothing to a missing node.
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
