use anyrender::PaintScene;

use crate::layout::{LayoutBoxTree, LayoutBoxKey, LayoutTy};

pub struct CometRenderer {}

impl CometRenderer {
    pub fn new() -> Self {
        Self {}
    }

    pub fn render(&mut self, tree: &LayoutBoxTree, id: LayoutBoxKey, scene: &mut impl PaintScene) {
        let Some(node) = tree.boxes.get(id) else {
            return;
        };

        match node.ty {
            LayoutTy::Block => {}
            LayoutTy::Inline(inline_box) => {}
        }
    }
}
