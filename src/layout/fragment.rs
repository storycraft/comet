use slotmap::{SlotMap, new_key_type};

use crate::{layout::BoxLayout, tree::slot::SlotTree, ui::NodeKey};

new_key_type! {
    pub struct LayoutBoxKey;
    pub struct InlineLayoutKey;
}

pub struct LayoutBoxTree {
    pub boxes: SlotTree<LayoutBoxKey, LayoutBox>,
    pub inline_layouts: SlotMap<InlineLayoutKey, parley::Layout<()>>,
}

impl LayoutBoxTree {
    pub fn new() -> Self {
        Self {
            boxes: SlotTree::new(),
            inline_layouts: SlotMap::with_key(),
        }
    }

    pub fn delete_box(&mut self, key: LayoutBoxKey) -> Option<LayoutBox> {
        let layout_box = self.boxes.delete_node(key)?;
        if let LayoutBoxTy::InlineBox { layout } = layout_box.ty {
            self.inline_layouts.remove(layout);
        }

        Some(layout_box)
    }
}

#[derive(Debug, Clone)]
/// A fully resolved layout box
pub struct LayoutBox {
    /// Optional span to connected [`NodeKey`]
    pub span: Option<NodeKey>,
    /// Type of this [`LayoutBox`]
    pub ty: LayoutBoxTy,
    /// Describe if it's part of singular / multiple layout boxes
    pub part: LayoutPart,
    /// Fully resolved layout relative to parent [`LayoutBox`]
    pub layout: Option<BoxLayout>,
}

impl LayoutBox {
    #[inline]
    pub const fn new(span: Option<NodeKey>, ty: LayoutBoxTy, part: LayoutPart) -> Self {
        Self::new_with_layout(span, ty, part, None)
    }

    #[inline]
    pub const fn new_with_layout(
        span: Option<NodeKey>,
        ty: LayoutBoxTy,
        part: LayoutPart,
        layout: Option<BoxLayout>,
    ) -> Self {
        Self {
            span,
            ty,
            part,
            layout,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LayoutBoxTy {
    Box,
    InlineBox {
        layout: InlineLayoutKey,
    },
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
pub enum LayoutPart {
    Full,
    Left,
    Right,
    Middle,
}
