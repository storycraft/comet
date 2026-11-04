mod adapter;
mod inline;
mod state;

pub mod conv;
pub mod style;

use comet_tree::{
    node::{NodeKey, Text},
    ui::Ui,
};
use rustc_hash::FxHashMap;
use slotmap::Key;

pub(crate) use state::NodeState;

use self::inline::{InlineEvents, Op};

use crate::{
    fragment::tree::FragmentTree,
    parley::default_text_style,
    tree::{ChildLayoutMode, LayoutItemKey, LayoutItemTy, LayoutTree},
};

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

        for key in tree.items.descendants(root) {
            let mut cx = constraint.clone();

            if let Some(style) =
                crate::parley::style::resolve_for_item(ui, tree, key, constraint, &mut resolved)
            {
                cx.font_size = style.font_size as f64;
            }

            let style = style::resolve(ui, tree.items[key].ty.node(), &cx);

            nodes.insert(key, NodeState::new(style));
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
        for key in self.tree.items.descendants(root) {
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

        let baseline = self.inline_baseline(root);
        self.nodes.get_mut(&root).unwrap().baseline = baseline;
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

    /// Baseline relative to the border-box top. The fallback is the bottom
    /// margin edge, which can lie outside the border box.
    pub(crate) fn inline_baseline(&self, key: LayoutItemKey) -> f32 {
        let state = &self.nodes[&key];

        if state.style.overflow.x == taffy::Overflow::Visible
            && state.style.overflow.y == taffy::Overflow::Visible
            && let Some(baseline) = self.last_baseline(key)
        {
            return baseline;
        }

        state.layout.size.height + state.layout.margin.bottom
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
        // Atomic children must be laid out before borrowing Parley's builder.
        for op in InlineEvents::new(self.tree, key) {
            if let Op::Box(child) = op {
                self.compute(child, available);
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

        for op in InlineEvents::new(self.tree, key) {
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
                    let layout = self.nodes[&child].layout;

                    // Parley places the margin box; emitted fragments retain
                    // border-box coordinates and dimensions.
                    builder.push_inline_box(parley::InlineBox {
                        id: child.data().as_ffi(),
                        index: 0,
                        width: layout.size.width + layout.margin.left + layout.margin.right,
                        height: layout.size.height + layout.margin.top + layout.margin.bottom,
                        kind: parley::InlineBoxKind::InFlow,
                        baseline: Some(layout.margin.top + self.nodes[&child].baseline),
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
