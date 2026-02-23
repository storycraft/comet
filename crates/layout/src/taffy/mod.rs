mod cache;
mod inline;
pub mod style;
mod traverse;

use crate::{
    cx::LayoutContext,
    layout_box::BoxLayout,
    taffy::{inline::InlineLayout, style::TaffyCoreStyle},
    tree::{LayoutNodeKey, LayoutNodeTy, LayoutTree},
};
use comet_div::ui::Ui;
use kurbo::Size;
use parley::FontContext;
use slotmap::{Key, KeyData};
use taffy::{
    AvailableSpace, LayoutBlockContainer, LayoutPartialTree, compute_block_layout,
    compute_cached_layout, compute_leaf_layout,
};

pub struct TaffyLayout<'a> {
    font_cx: &'a mut FontContext,
    cx: &'a mut LayoutContext,
    ui: &'a mut Ui,
    tree: &'a mut LayoutTree,
    root_size: Size,
}

impl<'a> TaffyLayout<'a> {
    pub fn layout(
        font_cx: &'a mut FontContext,
        cx: &'a mut LayoutContext,
        ui: &'a mut Ui,
        tree: &'a mut LayoutTree,
        root: LayoutNodeKey,
        root_size: Size,
        available_space: ::taffy::Size<AvailableSpace>,
    ) {
        Self {
            font_cx,
            cx,
            ui,
            tree,
            root_size,
        }
        .with_child(root, |this| {
            ::taffy::compute::compute_root_layout(this, to_taffy_key(root), available_space)
        })
    }

    fn with_child<R>(&mut self, parent: LayoutNodeKey, f: impl FnOnce(&mut Self) -> R) -> R {
        self.cx.buffer.push(self.tree, parent);
        f(&mut scopeguard::guard(self, |this| {
            this.cx.buffer.pop();
        }))
    }

    fn layout_child(
        &mut self,
        node_id: taffy::NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let id = from_taffy_key(node_id);
        let node = &mut self.tree.nodes[id];
        let need_reshape = node.cache.is_empty();
        match node.ty {
            LayoutNodeTy::Block(_) => {
                self.with_child(id, |this| compute_block_layout(this, node_id, inputs))
            }
            LayoutNodeTy::Inline(inline_node_key) => compute_leaf_layout(
                inputs,
                &taffy::Style::<String>::DEFAULT,
                |_, _| 0.0,
                |_, available_space| {
                    if need_reshape {
                        InlineLayout::new(
                            self.font_cx,
                            self.cx,
                            self.ui,
                            self.tree,
                            self.root_size,
                        )
                        .compute_layout(inline_node_key);
                    }

                    let available_size = available_space.width.into_option();
                    let inline_node = &mut self.tree.inline_nodes[inline_node_key];
                    inline_node.layout.break_all_lines(available_size);

                    taffy::Size {
                        width: inline_node.layout.full_width(),
                        height: inline_node.layout.height(),
                    }
                },
            ),
        }
    }
}

impl LayoutPartialTree for TaffyLayout<'_> {
    type CoreContainerStyle<'a>
        = TaffyCoreStyle<'a>
    where
        Self: 'a;
    type CustomIdent = String;

    fn get_core_container_style(&self, node_id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        core_style_of(self, node_id).unwrap_or_default()
    }

    fn set_unrounded_layout(&mut self, node_id: taffy::NodeId, layout: &taffy::Layout) {
        self.tree.nodes[from_taffy_key(node_id)].layout = BoxLayout::from_taffy_layout(*layout);
    }

    #[inline]
    fn compute_child_layout(
        &mut self,
        node_id: taffy::NodeId,
        inputs: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        compute_cached_layout(self, node_id, inputs, Self::layout_child)
    }
}

impl LayoutBlockContainer for TaffyLayout<'_> {
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

fn core_style_of<'a>(this: &'a TaffyLayout, node_id: taffy::NodeId) -> Option<TaffyCoreStyle<'a>> {
    let LayoutNodeTy::Block(span) = this.tree.nodes.get(from_taffy_key(node_id))?.ty else {
        return None;
    };

    Some(TaffyCoreStyle(this.ui.props(span?)))
}

pub fn from_taffy_key(id: taffy::NodeId) -> LayoutNodeKey {
    LayoutNodeKey::from(KeyData::from_ffi(id.into()))
}

pub fn to_taffy_key(id: LayoutNodeKey) -> taffy::NodeId {
    taffy::NodeId::new(id.data().as_ffi())
}
