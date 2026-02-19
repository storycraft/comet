pub(crate) mod children;

use comet_div::ui::{NodeKey, Ui};
use kurbo::Size;
use parley::FontContext;

use crate::{
    cx::children::ChildrenStack,
    taffy::TaffyLayout,
    tree::{LayoutNodeKey, LayoutTree},
};

pub struct LayoutContext {
    pub(crate) parley: parley::LayoutContext<Option<NodeKey>>,
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
        size: (f32, f32),
    ) {
        TaffyLayout::layout(
            font_cx,
            self,
            ui,
            tree,
            root,
            Size::new(size.0 as _, size.1 as _),
            taffy::Size {
                width: taffy::AvailableSpace::Definite(size.0),
                height: taffy::AvailableSpace::Definite(size.1),
            },
        );
    }
}

impl Default for LayoutContext {
    fn default() -> Self {
        Self::new()
    }
}
