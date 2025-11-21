pub mod builder;
pub mod cx;
mod inline;

use slotmap::{SlotMap, new_key_type};

use crate::{layout::BoxLayout, tree::slot::SlotTree, ui::NodeKey};

new_key_type! {
    pub struct LayoutBoxKey;
    pub struct InlineLayoutBoxKey;
    pub struct InlineLayoutKey;
}

pub struct LayoutBoxTree {
    pub boxes: SlotTree<LayoutBoxKey, LayoutBox>,
    pub inline_boxes: SlotTree<InlineLayoutBoxKey, InlineLayoutBox>,
    pub inline_layouts: SlotMap<InlineLayoutKey, parley::Layout<()>>,
}

impl LayoutBoxTree {
    pub fn new() -> Self {
        Self {
            boxes: SlotTree::new(),
            inline_boxes: SlotTree::new(),
            inline_layouts: SlotMap::with_key(),
        }
    }

    pub fn delete_box(&mut self, key: LayoutBoxKey) -> Option<LayoutBox> {
        let layout_box = self.boxes.delete_node(key)?;
        if let LayoutBoxTy::InlineBox {
            layout,
            inline_key: child_start,
        } = layout_box.ty
        {
            self.inline_layouts.remove(layout);

            let mut next_child = Some(child_start);
            while let Some(child) = next_child {
                next_child = self.inline_boxes.next_sibling(child);
                self.delete_inline_box(child);
            }
        }

        Some(layout_box)
    }

    pub fn delete_inline_box(&mut self, key: InlineLayoutBoxKey) -> Option<InlineLayoutBox> {
        let layout_box = self.inline_boxes.delete_node(key)?;
        if let InlineLayoutBoxTy::Box(id) = layout_box.ty {
            self.delete_box(id);
        }

        Some(layout_box)
    }
}

impl Default for LayoutBoxTree {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
/// A fully resolved layout box
pub struct LayoutBox {
    /// Optional span to connected [`NodeKey`]
    pub span: Option<NodeKey>,
    /// Type of this [`LayoutBox`]
    pub ty: LayoutBoxTy,
    /// Fully resolved layout relative to parent
    pub layout: BoxLayout,
}

impl LayoutBox {
    #[inline]
    pub fn new(span: Option<NodeKey>, ty: LayoutBoxTy) -> Self {
        Self {
            span,
            ty,
            layout: BoxLayout::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LayoutBoxTy {
    Box,
    InlineBox {
        layout: InlineLayoutKey,
        inline_key: InlineLayoutBoxKey,
    },
}

#[derive(Debug, Clone)]
pub struct InlineLayoutBox {
    /// Optional span to connected [`NodeKey`]
    pub span: Option<NodeKey>,
    /// Type of this [`InlineLayoutBox`]
    pub ty: InlineLayoutBoxTy,
    /// Fully resolved layout relative to parent
    pub layout: BoxLayout,
    /// Concatenated inline texts
    pub texts: String,
}

#[derive(Debug, Clone, Copy)]
pub enum InlineLayoutBoxTy {
    Box(LayoutBoxKey),
    InlineBox(InlineLayoutPart),
    LineBox {
        index: usize,
    },
    Text {
        run_index: usize,
        cluster_start: usize,
        cluster_end: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum InlineLayoutPart {
    Full,
    Left,
    Right,
    Middle,
}
