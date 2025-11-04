mod cache;
mod compute;
mod layout;
pub mod style;
mod traverse;

use slotmap::{Key, KeyData};
use taffy::{AvailableSpace, Size, compute_root_layout};

use crate::{
    Ui,
    layout::tree::{LayoutBoxKey, LayoutBoxTree},
};

pub struct TaffyLayoutImpl<'a> {
    layout_tree: &'a mut LayoutBoxTree,
    ui: &'a mut Ui,
}

impl<'a> TaffyLayoutImpl<'a> {
    pub fn new(layout_tree: &'a mut LayoutBoxTree, ui: &'a mut Ui) -> Self {
        Self { layout_tree, ui }
    }

    pub fn compute_layout(&mut self, root: LayoutBoxKey, available_space: Size<AvailableSpace>) {
        compute_root_layout(self, to_taffy_key(root), available_space);
    }
}

pub fn from_taffy_key(id: taffy::NodeId) -> LayoutBoxKey {
    LayoutBoxKey::from(KeyData::from_ffi(id.into()))
}

pub fn to_taffy_key(id: LayoutBoxKey) -> taffy::NodeId {
    taffy::NodeId::new(id.data().as_ffi())
}
