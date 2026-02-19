pub(crate) mod children;

use comet_div::ui::Ui;
use parley::FontContext;
use taffy::AvailableSpace;

use crate::{
    cx::children::ChildrenStack,
    taffy::TaffyLayout,
    tree::{LayoutNodeKey, LayoutTree},
};

pub struct LayoutContext {
    pub(crate) parley: parley::LayoutContext<()>,
    pub(crate) buffer: ChildrenStack,
}

impl LayoutContext {
    pub fn new() -> Self {
        Self {
            parley: parley::LayoutContext::new(),
            buffer: ChildrenStack::new(),
        }
    }

    #[inline]
    pub fn layout<'a>(
        &mut self,
        font_cx: &'a mut FontContext,
        ui: &'a mut Ui,
        tree: &'a mut LayoutTree,
        root: LayoutNodeKey,
        available_space: ::taffy::Size<AvailableSpace>,
    ) {
        TaffyLayout::layout(font_cx, self, ui, tree, root, available_space);
    }
}

impl Default for LayoutContext {
    fn default() -> Self {
        Self::new()
    }
}
