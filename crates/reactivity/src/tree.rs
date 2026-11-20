use comet_layout::{dirty_map::DirtyMap, input::tree::LayoutInputTree};
use comet_tree::{node::NodeKey, ui::Ui};

use crate::prop::ReactiveProp;

/// A [`Ui`] with the layout state its changes are marked in. Shared as
/// `Rc<RefCell<UiTree>>` between the application, which lays it out and
/// paints it, and reactive bindings, which write props into it.
///
/// Never hold a borrow across an executor poll or `.await`: bindings run as
/// executor tasks and borrow it mutably.
#[derive(Default)]
pub struct UiTree {
    pub ui: Ui,
    /// Layout items of `ui`, used to mark changes in `dirty`.
    pub inputs: LayoutInputTree,
    /// Changes not consumed by the layout yet.
    pub dirty: DirtyMap,
    /// Whether a binding changed a prop since the last
    /// [`take_redraw`](Self::take_redraw).
    redraw: bool,
}

impl UiTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a binding changed a prop since the last call, so a frame is
    /// needed. Layout changes are also marked in [`Self::dirty`].
    pub fn take_redraw(&mut self) -> bool {
        core::mem::take(&mut self.redraw)
    }

    /// Set a prop of an existing node, marking its [`ReactiveProp::DIRTY`]
    /// flags. Returns whether the prop changed: values equal to the current
    /// one and missing or reserved nodes are ignored.
    pub fn set_prop<P: ReactiveProp>(&mut self, node: NodeKey, prop: P) -> bool {
        match self.ui.props(node) {
            None => return false,
            Some(props) if props.get::<P>().is_some_and(|current| *current == prop) => {
                return false;
            }
            Some(_) => {}
        }

        self.ui.set_props(node, (prop,));
        // Nodes without an item are not laid out yet, so nothing is marked.
        self.dirty.mark_node(&self.inputs, node, P::DIRTY);
        self.redraw = true;

        true
    }
}
