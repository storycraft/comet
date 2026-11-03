pub mod conv;
pub mod style;

use comet_tree::{
    node::{NodeKey, Text},
    ui::Ui,
};
use rustc_hash::FxHashMap;
use slotmap::Key;
use taffy::{CacheTree, LayoutPartialTree, TraversePartialTree};

use crate::{
    fragment::tree::FragmentTree,
    parley::default_text_style,
    tree::{ChildLayoutMode, LayoutItemKey, LayoutItemTy, LayoutTree},
};

pub(crate) struct NodeState {
    pub style: taffy::Style,
    pub layout: taffy::Layout,
    pub cache: taffy::Cache,
    pub hoisted: Vec<taffy::NodeId>,
}

/// Adapter over layout items. Inline formatting contexts are measured by Parley.
pub struct TaffyLayout<'a> {
    pub(crate) ui: &'a Ui,
    pub(crate) tree: &'a LayoutTree,
    pub(crate) fragments: &'a mut FragmentTree,
    pub(crate) fonts: &'a mut parley::FontContext,
    pub(crate) parley: &'a mut parley::LayoutContext<Option<NodeKey>>,
    pub(crate) nodes: FxHashMap<LayoutItemKey, NodeState>,
}

pub use conv::{from_taffy_key, to_taffy_key};

impl<'a> TaffyLayout<'a> {
    /// Create a standalone adapter. `FragmentBuilder` additionally retains caches
    /// and reconciles the computed boxes into the final fragment tree.
    pub fn new(
        ui: &'a Ui,
        tree: &'a LayoutTree,
        fragments: &'a mut FragmentTree,
        fonts: &'a mut parley::FontContext,
        parley: &'a mut parley::LayoutContext<Option<NodeKey>>,
        constraint: &crate::style::LayoutConstraint,
        root: LayoutItemKey,
    ) -> Option<Self> {
        tree.items.get(root)?;
        let mut nodes = FxHashMap::default();
        let mut resolved = FxHashMap::default();
        let mut pending = vec![root];

        while let Some(key) = pending.pop() {
            let mut cx = constraint.clone();
            if let Some(style) =
                crate::parley::style::resolve_for_item(ui, tree, key, constraint, &mut resolved)
            {
                cx.font_size = style.font_size as f64;
            }

            let style = style::resolve(ui, tree.items[key].ty.node(), &cx);
            nodes.insert(
                key,
                NodeState {
                    style,
                    layout: Default::default(),
                    cache: Default::default(),
                    hoisted: Vec::new(),
                },
            );
            pending.extend(tree.items.children(key));
        }

        fragments.text_styles.extend(resolved);

        Some(Self {
            ui,
            tree,
            fragments,
            fonts,
            parley,
            nodes,
        })
    }

    pub fn layout(&self, key: LayoutItemKey) -> Option<&taffy::Layout> {
        self.nodes.get(&key).map(|node| &node.layout)
    }

    pub fn compute(&mut self, root: LayoutItemKey, available: taffy::Size<taffy::AvailableSpace>) {
        taffy::compute_root_layout(self, to_taffy_key(root), available);
        // Cached measurements may leave text at an intrinsic width. Finalize it
        // before a containing inline context reads this subtree's baseline.
        let mut pending = vec![root];
        while let Some(key) = pending.pop() {
            pending.extend(self.tree.items.children(key));
            if let Some(text) = self.fragments.inline_layouts.get_mut(&key) {
                let bx = self.nodes[&key].layout;
                let width = (bx.size.width
                    - bx.padding.left
                    - bx.padding.right
                    - bx.border.left
                    - bx.border.right)
                    .max(0.0);
                text.break_all_lines(Some(width));
            }
        }
    }

    /// Last in-flow line baseline, measured from the border-box top.
    fn last_baseline(&self, key: LayoutItemKey) -> Option<f32> {
        let bx = self.nodes[&key].layout;
        if let Some(text) = self.fragments.inline_layouts.get(&key) {
            return text
                .lines()
                .last()
                .map(|line| bx.border.top + bx.padding.top + line.metrics().baseline);
        }
        self.tree.items.children(key).rev().find_map(|child| {
            if self.nodes[&child].style.position == taffy::Position::Absolute {
                return None;
            }
            self.last_baseline(child)
                .map(|baseline| self.nodes[&child].layout.location.y + baseline)
        })
    }

    pub(crate) fn inline_baseline(&self, key: LayoutItemKey) -> f32 {
        let state = &self.nodes[&key];
        if state.style.overflow.x == taffy::Overflow::Visible
            && state.style.overflow.y == taffy::Overflow::Visible
            && let Some(baseline) = self.last_baseline(key)
        {
            return baseline;
        }
        state.layout.size.height
    }

    fn layout_child(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        let key = from_taffy_key(id);
        let item = self.tree.items[key];

        if item.child_layout == ChildLayoutMode::Block {
            return taffy::compute_block_layout(self, id, input, block_ctx);
        }

        let style = self.nodes[&key].style.clone();
        let inline = item.child_layout == ChildLayoutMode::Inline
            || matches!(item.ty, LayoutItemTy::Text(_));
        taffy::compute_leaf_layout(
            input,
            &style,
            |_, _| 0.0,
            |known, available| {
                if !inline {
                    return taffy::Size::ZERO;
                }

                let width = known.width.or(match available.width {
                    taffy::AvailableSpace::Definite(v) => Some(v),
                    taffy::AvailableSpace::MinContent => Some(0.0),
                    taffy::AvailableSpace::MaxContent => None,
                });
                self.shape(key, available);
                let layout = self.fragments.inline_layouts.get_mut(&key).unwrap();
                layout.break_all_lines(width);

                taffy::Size {
                    width: layout.width(),
                    height: layout.height(),
                }
            },
        )
    }

    fn shape(&mut self, key: LayoutItemKey, available: taffy::Size<taffy::AvailableSpace>) {
        enum Op {
            Push(NodeKey),
            Pop,
            Text(NodeKey),
            Box(LayoutItemKey),
            Empty(LayoutItemKey),
        }

        fn collect(tree: &LayoutTree, key: LayoutItemKey, ops: &mut Vec<Op>) {
            match tree.items[key].ty {
                LayoutItemTy::Text(node) => ops.push(Op::Text(node)),
                LayoutItemTy::AtomicInline(_) => ops.push(Op::Box(key)),
                _ => {
                    let node = tree.items[key].ty.node();
                    if let Some(node) = node {
                        ops.push(Op::Push(node));
                    }
                    for child in tree.items.children(key) {
                        collect(tree, child, ops);
                    }

                    if tree.items.first_child(key).is_none() {
                        ops.push(Op::Empty(key));
                    }

                    if node.is_some() {
                        ops.push(Op::Pop);
                    }
                }
            }
        }

        let mut ops = Vec::new();
        if let LayoutItemTy::Text(node) = self.tree.items[key].ty {
            ops.push(Op::Text(node));
        } else {
            for child in self.tree.items.children(key) {
                collect(self.tree, child, &mut ops);
            }
        }

        let mut baselines = FxHashMap::default();
        for op in &ops {
            if let Op::Box(child) = op {
                self.compute(*child, available);
                baselines.insert(*child, self.inline_baseline(*child));
            }
        }

        let mut ancestor = Some(key);
        let mut source = None;
        while let Some(key) = ancestor {
            source = self.tree.items[key].ty.node();
            if source.is_some() {
                break;
            }
            ancestor = self.tree.items.parent(key);
        }

        let root_style = source
            .and_then(|node| self.fragments.text_styles.get(&node))
            .cloned()
            .unwrap_or_else(default_text_style);
        let mut builder = self
            .parley
            .tree_builder(self.fonts, 1.0, false, &root_style);

        for op in ops {
            match op {
                Op::Push(node) => {
                    builder.push_style_span(self.fragments.text_styles[&node].clone())
                }
                Op::Pop => builder.pop_style_span(),
                Op::Text(node) => {
                    builder.push_style_span(self.fragments.text_styles[&node].clone());
                    if let Some(text) = self.ui.prop::<Text>(node) {
                        builder.push_text(&text.0);
                    }
                    builder.pop_style_span();
                }
                Op::Box(child) => {
                    let size = self.nodes[&child].layout.size;
                    builder.push_inline_box(parley::InlineBox {
                        id: child.data().as_ffi(),
                        index: 0,
                        width: size.width,
                        height: size.height,
                        kind: parley::InlineBoxKind::InFlow,
                        baseline: Some(baselines[&child]),
                        vertical_align: Default::default(),
                    });
                }
                Op::Empty(child) => {
                    builder.push_inline_box(parley::InlineBox {
                        id: child.data().as_ffi(),
                        index: 0,
                        width: 0.0,
                        height: 0.0,
                        kind: parley::InlineBoxKind::InFlow,
                        baseline: None,
                        vertical_align: Default::default(),
                    });
                }
            }
        }

        let (layout, text) = builder.build();
        self.fragments.inline_layouts.insert(key, layout);
        self.fragments.texts.insert(key, text);
    }
}

impl TraversePartialTree for TaffyLayout<'_> {
    type ChildIter<'a>
        = std::vec::IntoIter<taffy::NodeId>
    where
        Self: 'a;

    fn child_ids(&self, id: taffy::NodeId) -> Self::ChildIter<'_> {
        let key = from_taffy_key(id);
        if self.tree.items[key].child_layout != ChildLayoutMode::Block {
            return Vec::new().into_iter();
        }

        self.tree
            .items
            .children(key)
            .map(to_taffy_key)
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn child_count(&self, id: taffy::NodeId) -> usize {
        self.child_ids(id).len()
    }

    fn get_child_id(&self, id: taffy::NodeId, index: usize) -> taffy::NodeId {
        self.child_ids(id).nth(index).unwrap()
    }
}

impl LayoutPartialTree for TaffyLayout<'_> {
    type CoreContainerStyle<'a>
        = &'a taffy::Style
    where
        Self: 'a;
    type CustomIdent = String;

    fn get_core_container_style(&self, id: taffy::NodeId) -> Self::CoreContainerStyle<'_> {
        &self.nodes[&from_taffy_key(id)].style
    }

    fn set_unrounded_layout(&mut self, id: taffy::NodeId, layout: &taffy::Layout) {
        self.nodes.get_mut(&from_taffy_key(id)).unwrap().layout = *layout;
    }

    fn compute_child_layout(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        taffy::compute_cached_layout(self, id, input, |this, id, input| {
            this.layout_child(id, input, None)
        })
    }
}

impl taffy::LayoutBlockContainer for TaffyLayout<'_> {
    type BlockContainerStyle<'a>
        = &'a taffy::Style
    where
        Self: 'a;
    type BlockItemStyle<'a>
        = &'a taffy::Style
    where
        Self: 'a;

    fn get_block_container_style(&self, id: taffy::NodeId) -> Self::BlockContainerStyle<'_> {
        self.get_core_container_style(id)
    }

    fn get_block_child_style(&self, id: taffy::NodeId) -> Self::BlockItemStyle<'_> {
        self.get_core_container_style(id)
    }

    fn compute_block_child_layout(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        taffy::compute_cached_layout(self, id, input, |this, id, input| {
            this.layout_child(id, input, block_ctx)
        })
    }
}

impl taffy::LayoutContainingBlock for TaffyLayout<'_> {
    type OofItemStyle<'a>
        = &'a taffy::Style
    where
        Self: 'a;

    fn get_oof_item_style(&self, id: taffy::NodeId) -> Self::OofItemStyle<'_> {
        self.get_core_container_style(id)
    }

    fn clear_hoisted_children(&mut self, id: taffy::NodeId) {
        self.nodes
            .get_mut(&from_taffy_key(id))
            .unwrap()
            .hoisted
            .clear();
    }

    fn add_hoisted_children(&mut self, id: taffy::NodeId, children: &[taffy::NodeId]) {
        self.nodes
            .get_mut(&from_taffy_key(id))
            .unwrap()
            .hoisted
            .extend_from_slice(children);
    }
}

impl CacheTree for TaffyLayout<'_> {
    fn cache_get(
        &mut self,
        id: taffy::NodeId,
        input: &taffy::LayoutInput,
    ) -> Option<taffy::LayoutOutput> {
        self.nodes
            .get_mut(&from_taffy_key(id))
            .unwrap()
            .cache
            .get(input)
    }

    fn cache_store(
        &mut self,
        id: taffy::NodeId,
        input: &taffy::LayoutInput,
        output: taffy::LayoutOutput,
    ) {
        self.nodes
            .get_mut(&from_taffy_key(id))
            .unwrap()
            .cache
            .store(input, output);
    }

    fn cache_clear(&mut self, id: taffy::NodeId) {
        self.nodes
            .get_mut(&from_taffy_key(id))
            .unwrap()
            .cache
            .clear();
    }
}
