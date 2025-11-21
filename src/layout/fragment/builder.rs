mod inline;

use parley::{ClusterPath, FontContext};
use slotmap::SecondaryMap;

use crate::{
    layout::{
        fragment::{InlineLayoutBoxKey, LayoutBoxKey},
        input::{InputNodeKey, LayoutInputTree},
    },
    ui::{NodeKey, Ui},
};

pub struct LayoutBoxTreeBuilderContext {
    inline_stack: Vec<InlineState>,
    inline_layout: parley::LayoutContext<()>,
    current_inline: Option<(InlineLayoutBoxKey, InlineLayoutBoxKey)>,
    mappings: SecondaryMap<InputNodeKey, LayoutBoxKey>,
}

impl LayoutBoxTreeBuilderContext {
    pub fn new() -> Self {
        Self {
            inline_stack: vec![],
            inline_layout: parley::LayoutContext::new(),
            current_inline: None,
            mappings: SecondaryMap::new(),
        }
    }

    pub fn update(
        &mut self,
        font_cx: &mut FontContext,
        tree: &LayoutInputTree,
        node: InputNodeKey,
    ) {
    }

    pub fn build(&mut self, font_cx: &mut FontContext, tree: &LayoutInputTree) {}
}

impl Default for LayoutBoxTreeBuilderContext {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
struct InlineState {
    pub span: NodeKey,
    pub index: usize,
}

struct LayoutBoxTreeBuilder<'a> {
    cx: &'a mut LayoutBoxTreeBuilderContext,
    font_cx: &'a mut FontContext,
    ui: &'a Ui,
}

impl<'a> LayoutBoxTreeBuilder<'a> {
    pub fn layout(self) {}
}
