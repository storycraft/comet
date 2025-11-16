pub mod builder;

use slotmap::SlotMap;

use crate::{
    layout::{InlineBox, InlineBoxKey, InlineIns, InlineKey, LayoutBox, LayoutBoxKey, LayoutTy},
    tree::slot::SlotTree,
};

pub struct LayoutBoxTree {
    pub boxes: SlotTree<LayoutBoxKey, LayoutBox>,
    pub inline_boxes: SlotMap<InlineBoxKey, InlineBox>,
    pub inlines: SlotTree<InlineKey, InlineIns>,
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

    pub fn delete_layout_box(&mut self, key: LayoutBoxKey) -> Option<LayoutBox> {
        let layout_box = self.boxes.delete_node(key)?;
        match layout_box.ty {
            LayoutTy::Block => {}
            LayoutTy::Inline(key) => {
                self.delete_inline_box(key);
            }
        }

        Some(layout_box)
    }

    pub fn delete_inline_box(&mut self, key: InlineBoxKey) -> Option<InlineBox> {
        let inline_box = self.inline_boxes.remove(key)?;
        if let Some(inline) = inline_box.inline_start {
            self.inlines.delete_node(inline);
        }
        Some(inline_box)
    }
}
