mod adapter;
mod inline;
mod state;

#[cfg(test)]
mod tests;

pub mod conv;
pub mod style;

use comet_tree::{
    node::{NodeKey, Text},
    ui::Ui,
};
use rustc_hash::FxHashMap;
use slotmap::{Key, SecondaryMap};
use taffy::style::CoreStyle;

pub(crate) use state::NodeState;

use self::inline::{InlineEvents, Op};

use crate::{
    fragment::{FragmentTree, InlineLayout},
    parley::default_text_style,
    style::bx::DisplayInner,
    tree::{ChildLayoutMode, LayoutItemKey, LayoutItemTy, LayoutTree},
};

/// Adapter over layout items. Inline formatting contexts are measured by Parley.
pub struct TaffyLayout<'a> {
    pub(crate) constraint: crate::style::LayoutConstraint,
    pub(crate) ui: &'a Ui,
    pub(crate) tree: &'a LayoutTree,
    pub(crate) fragments: &'a mut FragmentTree,
    pub(crate) fonts: &'a mut parley::FontContext,
    pub(crate) parley: &'a mut parley::LayoutContext<Option<NodeKey>>,
    pub(crate) nodes: SecondaryMap<LayoutItemKey, NodeState>,
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

        let mut nodes = SecondaryMap::new();
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
            constraint: constraint.clone(),
            ui,
            tree,
            fragments,
            fonts,
            parley,
            nodes,
        })
    }

    pub fn layout(&self, key: LayoutItemKey) -> Option<&taffy::Layout> {
        self.nodes.get(key).map(|node| &node.layout)
    }

    pub fn compute(&mut self, root: LayoutItemKey, available: taffy::Size<taffy::AvailableSpace>) {
        taffy::compute_root_layout(self, to_taffy_key(root), available);

        let baseline = self.inline_baseline(root);
        self.nodes.get_mut(root).unwrap().baseline = baseline;
    }

    /// Last in-flow line baseline, measured from the border-box top.
    fn last_baseline(&self, key: LayoutItemKey) -> Option<f32> {
        let bx = self.nodes[key].layout;

        if let Some(text) = self.fragments.inline_layouts.get(key) {
            return text
                .layout
                .lines()
                .last()
                .map(|line| bx.border.top + bx.padding.top + line.metrics().baseline);
        }

        self.tree.items.children(key).rev().find_map(|child| {
            if self.nodes[child].style.position() == taffy::Position::Absolute {
                return None;
            }

            self.last_baseline(child)
                .map(|baseline| self.nodes[child].layout.location.y + baseline)
        })
    }

    /// Baseline relative to the border-box top. The fallback is the bottom
    /// margin edge, which can lie outside the border box.
    pub(crate) fn inline_baseline(&self, key: LayoutItemKey) -> f32 {
        let state = &self.nodes[key];

        if state.style.overflow().x == taffy::Overflow::Visible
            && state.style.overflow().y == taffy::Overflow::Visible
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

        match item.child_layout {
            ChildLayoutMode::Block => {
                return taffy::compute_block_layout(self, id, input, block_ctx);
            }
            ChildLayoutMode::Container => {
                let ui = self.ui;
                let inner = item
                    .ty
                    .node()
                    .and_then(|node| ui.prop::<DisplayInner>(node));

                let Some(DisplayInner::Container(algorithm)) = inner.as_deref() else {
                    return taffy::LayoutOutput::DEFAULT;
                };

                let children = self.tree.items.children(key).collect();
                let mut context = crate::container::ContainerContext {
                    engine: self,
                    root: key,
                    children,
                };

                return algorithm.layout(&mut context, input);
            }

            ChildLayoutMode::Inline => self.layout_inline(key, input),
            ChildLayoutMode::None if matches!(item.ty, LayoutItemTy::Text(_)) => {
                self.layout_inline(key, input)
            }

            _ => return taffy::LayoutOutput::HIDDEN,
        }
    }

    fn layout_inline(
        &mut self,
        key: LayoutItemKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let style = self.nodes[key].style.clone();
        let mut output = taffy::compute_leaf_layout(
            input,
            &style,
            |_, _| 0.0,
            |_, available| self.measure_inline(key, available),
        );

        if let Some(text) = self.fragments.inline_layouts.get(key) {
            use taffy::ResolveOrZero;

            let text = &text.layout;

            let padding = style
                .padding()
                .resolve_or_zero(input.parent_size.width, |_, _| 0.0);
            let border = style
                .border()
                .resolve_or_zero(input.parent_size.width, |_, _| 0.0);
            let top = padding.top + border.top;

            output.baselines = taffy::Baselines {
                first: text
                    .lines()
                    .next()
                    .map(|line| top + line.metrics().baseline),
                last: text
                    .lines()
                    .next_back()
                    .map(|line| top + line.metrics().baseline),
            };
        }

        output
    }

    fn measure_inline(
        &mut self,
        key: LayoutItemKey,
        available: taffy::Size<taffy::AvailableSpace>,
    ) -> taffy::Size<f32> {
        let width = match available.width {
            taffy::AvailableSpace::Definite(v) => Some(v),
            taffy::AvailableSpace::MinContent => Some(0.0),
            taffy::AvailableSpace::MaxContent => None,
        };

        self.shape(key, available);

        let layout = &mut self.fragments.inline_layouts.get_mut(key).unwrap().layout;
        layout.break_all_lines(width);

        if available.width == taffy::AvailableSpace::MinContent {
            // Zero width determines the intrinsic minimum. Measure height at
            // that width so short words can share a line in the final layout.
            layout.break_all_lines(Some(layout.width()));
        }

        taffy::Size {
            width: layout.width(),
            height: layout.height(),
        }
    }

    fn shape(&mut self, key: LayoutItemKey, available: taffy::Size<taffy::AvailableSpace>) {
        // Shaping may replace final text and atomic descendant geometry. Size
        // caches remain useful, but ancestors must run final layout again.
        let mut ancestor = Some(key);
        while let Some(key) = ancestor {
            let Some(state) = self.nodes.get_mut(key) else {
                break;
            };

            state.final_layout_valid = false;
            ancestor = self.tree.items.parent(key);
        }

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
                    let layout = self.nodes[child].layout;

                    // Parley places the margin box; emitted fragments retain
                    // border-box coordinates and dimensions.
                    builder.push_inline_box(parley::InlineBox {
                        id: child.data().as_ffi(),
                        index: 0,
                        width: layout.size.width + layout.margin.left + layout.margin.right,
                        height: layout.size.height + layout.margin.top + layout.margin.bottom,
                        kind: parley::InlineBoxKind::InFlow,
                        baseline: Some(layout.margin.top + self.nodes[child].baseline),
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

        if let Some(inline) = self.fragments.inline_layouts.get_mut(key) {
            inline.text = builder.build_into(&mut inline.layout);
        } else {
            let (layout, text) = builder.build();

            self.fragments
                .inline_layouts
                .insert(key, InlineLayout { text, layout });
        }
    }
}
