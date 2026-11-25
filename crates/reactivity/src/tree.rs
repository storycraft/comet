use comet_tree::{node::NodeKey, ui::Ui};
use rustc_hash::FxHashMap;

use crate::prop::{PropHint, ReactiveProp};

/// A [`Ui`] with the changes made to it since they were last taken. Shared
/// as `Rc<RefCell<UiTree>>` between the application, which takes the
/// changes to lay out and paint them, and reactive bindings, which write
/// props into it.
///
/// Never hold a borrow across an executor poll or `.await`: bindings run as
/// executor tasks and borrow it mutably.
#[derive(Default)]
pub struct UiTree {
    pub ui: Ui,
    /// Flags of the nodes changed since the last
    /// [`take_changes`](Self::take_changes).
    changes: FxHashMap<NodeKey, PropHint>,
}

impl UiTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// The nodes changed since the changes were last taken, with what their
    /// changes invalidate. Nodes may have been removed since.
    pub fn changes(&self) -> &FxHashMap<NodeKey, PropHint> {
        &self.changes
    }

    /// Take the changes, for the consumer to process them.
    pub fn take_changes(&mut self) -> impl Iterator<Item = (NodeKey, PropHint)> + '_ {
        self.changes.drain()
    }

    /// Record `flags` for `node`, adding them to its earlier flags.
    pub fn mark(&mut self, node: NodeKey, flags: PropHint) {
        if !flags.is_empty() {
            *self.changes.entry(node).or_default() |= flags;
        }
    }

    /// Set a prop of an existing node, recording the flags
    /// [`ReactiveProp::hint`] gives for `prop` and its current value, if any.
    /// Returns whether the prop changed: values equal to the current one and
    /// missing or reserved nodes are ignored.
    pub fn set_prop<P: ReactiveProp>(&mut self, node: NodeKey, prop: P) -> bool {
        let hint = match self.ui.props(node) {
            None => return false,
            Some(props) => match props.get::<P>() {
                Some(current) if *current == prop => return false,
                old => prop.hint(old.as_deref()),
            },
        };

        self.ui.set_props(node, (prop,));
        self.mark(node, hint);

        true
    }
}
