mod cache;
mod inline;
pub mod style;
mod traverse;

use crate::{
    layout::{
        BoxLayout, LayoutBoxKey, LayoutContext, LayoutTy,
        taffy::{inline::InlineLayout, style::TaffyCoreStyle},
        tree::LayoutBoxTree,
    },
    ui::Ui,
};
use parley::{Alignment, AlignmentOptions, FontContext};
use slotmap::{Key, KeyData};
use taffy::{
    LayoutBlockContainer, LayoutPartialTree, compute_block_layout, compute_cached_layout,
    compute_leaf_layout,
};

pub(super) struct TaffyLayoutImpl<'a> {
    pub cx: &'a mut LayoutContext,
    pub ui: &'a Ui,
    pub tree: &'a mut LayoutBoxTree,
}

impl LayoutPartialTree for TaffyLayoutImpl<'_> {
    type CoreContainerStyle<'a>
        = TaffyCoreStyle<'a>
    where
        Self: 'a;
    type CustomIdent = String;

    fn get_core_container_style(&self, node_id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        core_style_of(self, node_id).unwrap_or_default()
    }

    fn set_unrounded_layout(&mut self, node_id: taffy::NodeId, layout: &taffy::Layout) {
        self.tree.boxes[from_taffy_key(node_id)].layout = BoxLayout::from_taffy_layout(*layout);
    }

    #[inline]
    fn compute_child_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        compute_cached_layout(self, node_id, inputs, Self::compute_uncached_layout)
    }
}

impl TaffyLayoutImpl<'_> {
    fn compute_uncached_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let id = from_taffy_key(node_id);
        let node = &mut self.tree.boxes[id];

        match node.ty {
            LayoutTy::Block => compute_block_layout(self, node_id, inputs),
            LayoutTy::Inline(inline_box_key) => compute_leaf_layout(
                inputs,
                &taffy::Style::<String>::DEFAULT,
                |_, _| 0.0,
                |_, available_space| {
                    // TODO:: move font context
                    InlineLayout::new(&mut FontContext::new(), self).compute_layout(inline_box_key);

                    let available_size = available_space.width.into_option();
                    let inline_box = &mut self.tree.inline_boxes[inline_box_key];
                    inline_box.parley_layout.break_all_lines(available_size);
                    inline_box.parley_layout.align(
                        available_size,
                        Alignment::Start,
                        AlignmentOptions::default(),
                    );

                    taffy::Size {
                        width: inline_box.parley_layout.full_width(),
                        height: inline_box.parley_layout.height(),
                    }
                },
            ),
        }
    }
}

impl LayoutBlockContainer for TaffyLayoutImpl<'_> {
    type BlockContainerStyle<'a>
        = TaffyCoreStyle<'a>
    where
        Self: 'a;

    type BlockItemStyle<'a>
        = TaffyCoreStyle<'a>
    where
        Self: 'a;

    fn get_block_container_style(&self, node_id: taffy::NodeId) -> Self::BlockContainerStyle<'_> {
        core_style_of(self, node_id).unwrap_or_default()
    }

    fn get_block_child_style(&self, child_node_id: taffy::NodeId) -> Self::BlockItemStyle<'_> {
        core_style_of(self, child_node_id).unwrap_or_default()
    }
}

fn core_style_of<'a>(
    this: &'a TaffyLayoutImpl,
    node_id: taffy::NodeId,
) -> Option<TaffyCoreStyle<'a>> {
    let span = this.tree.boxes[from_taffy_key(node_id)].span?;
    Some(TaffyCoreStyle(this.ui.props(span)))
}

pub fn from_taffy_key(id: taffy::NodeId) -> LayoutBoxKey {
    LayoutBoxKey::from(KeyData::from_ffi(id.into()))
}

pub fn to_taffy_key(id: LayoutBoxKey) -> taffy::NodeId {
    taffy::NodeId::new(id.data().as_ffi())
}
