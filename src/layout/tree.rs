pub mod builder;

use slotmap::SlotMap;
use taffy::{AvailableSpace, Size};

use crate::{
    layout::{
        InlineBox, InlineBoxKey, InlineItem, InlineKey, LayoutBox, LayoutBoxKey, LayoutTy,
        taffy::TaffyLayoutImpl,
    },
    tree::slot::SlotTree,
    ui::Ui,
};

pub struct LayoutBoxTree {
    pub boxes: SlotTree<LayoutBoxKey, LayoutBox>,
    pub inline_boxes: SlotMap<InlineBoxKey, InlineBox>,
    pub inlines: SlotTree<InlineKey, InlineItem>,
}

impl Default for LayoutBoxTree {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutBoxTree {
    pub fn new() -> Self {
        Self {
            boxes: SlotTree::new(),
            inline_boxes: SlotMap::with_key(),
            inlines: SlotTree::new(),
        }
    }

    pub fn clear(&mut self) {
        self.boxes.clear();
        self.inline_boxes.clear();
        self.inlines.clear();
    }

    pub fn create_root_box(&mut self) -> LayoutBoxKey {
        self.boxes.insert(LayoutBox::new(None, LayoutTy::Block))
    }

    pub fn compute_layout(
        &mut self,
        ui: &mut Ui,
        root: LayoutBoxKey,
        available_space: Size<AvailableSpace>,
    ) {
        TaffyLayoutImpl::new(self, ui).compute_layout(root, available_space);
    }
}
