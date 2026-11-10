use comet_tree::{
    node::{NodeKey, Text},
    ui::Ui,
};
use rustc_hash::FxHashMap;
use slotmap::{Key, KeyData, SparseSecondaryMap};
use taffy::{ResolveOrZero, style::CoreStyle};

use super::{
    Fragment, FragmentBox, FragmentItem, FragmentKey, InlineLayout,
    inline::{InlineEvents, Op},
    state::NodeState,
    tree::FragmentTree,
};
use crate::{
    container::ContainerContext,
    input::{ChildLayout, LayoutFlags, LayoutInputKey, LayoutInputKind, tree::LayoutInputTree},
    style::{
        LayoutConstraint,
        bx::DisplayInner,
        geometry::{Point, Size},
    },
    taffy::style::{self, Style},
};

pub(crate) fn to_taffy_key(key: LayoutInputKey) -> taffy::NodeId {
    taffy::NodeId::new(key.data().as_ffi())
}

pub(super) fn from_taffy_key(id: taffy::NodeId) -> LayoutInputKey {
    KeyData::from_ffi(id.into()).into()
}

fn from_box_id(id: u64) -> LayoutInputKey {
    KeyData::from_ffi(id).into()
}

/// Whether the item lays out its children in an inline formatting context.
///
/// Block containers with any inline level child establish one; block children
/// then interrupt the inline content instead of being wrapped in anonymous boxes.
pub(super) fn is_inline_root(tree: &LayoutInputTree, key: LayoutInputKey) -> bool {
    let item = &tree.items[key];

    match item.child_layout {
        ChildLayout::None => item.kind == LayoutInputKind::Text,
        ChildLayout::Inline => true,
        ChildLayout::Block => tree
            .items
            .children(key)
            .any(|child| tree.items[child].kind != LayoutInputKind::Block),
        ChildLayout::Container => false,
    }
}

/// Buffers retained by the builder between builds.
#[derive(Default)]
pub(crate) struct Scratch {
    stack: Vec<LayoutInputKey>,
    events: Vec<Placed>,
    spans: Vec<FragmentKey>,
    span_parents: FxHashMap<LayoutInputKey, FragmentKey>,
    links: FxHashMap<LayoutInputKey, FragmentKey>,
    /// Box fragments detached from their parents during this build. Those left
    /// without a parent at the end are deleted.
    detached: Vec<FragmentKey>,
}

/// Content placed by the line breaker, in flow order.
enum Placed {
    /// Index of a line in the Parley layout.
    Line(usize),
    /// Block box interrupting the inline content, with the top of its margin box
    /// relative to the content box.
    Block { key: LayoutInputKey, top: f32 },
}

/// Result of breaking an inline formatting context into lines.
struct Flow {
    width: f32,
    height: f32,
}

pub(crate) struct Engine<'a> {
    pub ui: &'a Ui,
    pub tree: &'a LayoutInputTree,
    pub fragments: &'a mut FragmentTree,
    pub fonts: &'a mut parley::FontContext,
    pub parley: &'a mut parley::LayoutContext<Option<NodeKey>>,
    pub constraint: &'a LayoutConstraint,
    pub root: LayoutInputKey,
    pub scratch: &'a mut Scratch,
}

impl Engine<'_> {
    /// Block level and atomic inline items, the root, and every child of a
    /// container generate a box.
    fn is_box(&self, key: LayoutInputKey) -> bool {
        key == self.root
            || matches!(
                self.tree.items[key].kind,
                LayoutInputKind::Block | LayoutInputKind::AtomicInline
            )
            || self.tree.items.parent(key).is_some_and(|parent| {
                self.tree.items[parent].child_layout == ChildLayout::Container
            })
    }

    fn resolve_style(&self, key: LayoutInputKey) -> Style {
        let item = &self.tree.items[key];
        let mut cx = self.constraint.clone();
        cx.font_size = item.text_style.font_size as f64;

        style::resolve(self.ui, Some(item.node), &cx)
    }

    /// Create the state and box fragment of a box generating item if missing.
    pub fn ensure_state(&mut self, key: LayoutInputKey) {
        let valid = self
            .fragments
            .states
            .get(key)
            .is_some_and(|state| self.fragments.items.get(state.fragment).is_some());

        if valid {
            return;
        }

        let style = self.resolve_style(key);
        let fragment = self.fragments.items.insert(Fragment::new(
            key,
            FragmentItem::Box,
            FragmentBox::default(),
        ));

        self.fragments
            .states
            .insert(key, NodeState::new(style, fragment));
    }

    /// Resolve the box style again and drop cached layouts of a flagged item.
    fn refresh_state(&mut self, key: LayoutInputKey) {
        let style = self.resolve_style(key);
        self.ensure_state(key);

        let state = &mut self.fragments.states[key];
        state.style = style;
        state.cache.clear();
        state.final_layout_valid = false;
    }

    /// Visit flagged items from the root. Cached layouts of flagged items are
    /// dropped and their fragment children are synchronized. Flags are kept.
    pub fn invalidate(&mut self) {
        // Without retained fragments every item is treated as flagged.
        let force = self.fragments.box_fragment(self.root).is_none();
        let mut stack = std::mem::take(&mut self.scratch.stack);
        stack.push(self.root);

        while let Some(key) = stack.pop() {
            let item = &self.tree.items[key];
            let flags = item.flags;
            let child_layout = item.child_layout;

            if !force && flags.is_empty() {
                continue;
            }

            if self.is_box(key) {
                self.refresh_state(key);

                let fragment = self.fragments.states[key].fragment;
                let inline = is_inline_root(self.tree, key);

                if inline {
                    if let Some(layout) = self.fragments.inline_layouts.get_mut(key) {
                        layout.shaped = false;
                    }
                } else {
                    self.fragments.inline_layouts.remove(key);
                }

                // Inline content is attached again when the inline layout is emitted.
                self.release_children(fragment);

                if !inline && matches!(child_layout, ChildLayout::Block | ChildLayout::Container) {
                    for child in self.tree.items.children(key) {
                        self.ensure_state(child);

                        let child = self.fragments.states[child].fragment;
                        self.fragments.items.append(fragment, child);
                    }
                }
            } else if let Some(state) = self.fragments.states.remove(key) {
                // The item no longer generates a box.
                self.fragments.inline_layouts.remove(key);
                self.release(state.fragment);
            }

            if (force || flags.contains(LayoutFlags::CHILDREN_DIRTY))
                && child_layout != ChildLayout::None
            {
                stack.extend(self.tree.items.children(key));
            }
        }

        self.scratch.stack = stack;
    }

    /// Whether a box fragment is the current box of a live box generating item.
    fn is_live_box(&self, fragment: FragmentKey) -> bool {
        let source = self.fragments.items[fragment].source;

        self.tree.items.get(source).is_some()
            && self.is_box(source)
            && self
                .fragments
                .states
                .get(source)
                .is_some_and(|state| state.fragment == fragment)
    }

    /// Delete the descendants of a fragment. Live box fragments are detached
    /// and kept for reuse; state of dead boxes is dropped.
    pub fn release_children(&mut self, fragment: FragmentKey) {
        let mut pending: Vec<FragmentKey> = self.fragments.items.children(fragment).collect();
        let mut live = Vec::new();

        while let Some(key) = pending.pop() {
            if self.fragments.items[key].item == FragmentItem::Box {
                if self.is_live_box(key) {
                    live.push(key);
                    continue;
                }

                let source = self.fragments.items[key].source;
                if self
                    .fragments
                    .states
                    .get(source)
                    .is_some_and(|state| state.fragment == key)
                {
                    self.fragments.states.remove(source);
                    self.fragments.inline_layouts.remove(source);
                }
            } else {
                let source = self.fragments.items[key].source;

                if self.fragments.first.get(source) == Some(&key) {
                    self.fragments.first.remove(source);
                }
            }

            pending.extend(self.fragments.items.children(key));
        }

        for key in live {
            self.fragments.items.remove_parent(key);
            self.scratch.detached.push(key);
        }

        while let Some(child) = self.fragments.items.first_child(fragment) {
            self.fragments.items.delete_node(child);
        }
    }

    /// Delete a fragment, keeping its live box descendants.
    pub fn release(&mut self, fragment: FragmentKey) {
        if self.fragments.items.get(fragment).is_none() {
            return;
        }

        self.release_children(fragment);
        self.fragments.items.delete_node(fragment);
    }

    /// Delete box fragments that were detached and never attached again.
    pub fn sweep(&mut self) {
        let root = self.fragments.box_fragment(self.root);

        while let Some(fragment) = self.scratch.detached.pop() {
            if self.fragments.items.get(fragment).is_none()
                || self.fragments.items.parent(fragment).is_some()
                || Some(fragment) == root
            {
                continue;
            }

            let source = self.fragments.items[fragment].source;
            if self
                .fragments
                .states
                .get(source)
                .is_some_and(|state| state.fragment == fragment)
            {
                self.fragments.states.remove(source);
                self.fragments.inline_layouts.remove(source);
            }

            self.release(fragment);
        }
    }

    /// Lay out a box as an independent root of the available space.
    pub fn compute(&mut self, key: LayoutInputKey, available: taffy::Size<taffy::AvailableSpace>) {
        taffy::compute_root_layout(self, to_taffy_key(key), available);

        if key == self.root || self.tree.items[key].kind == LayoutInputKind::AtomicInline {
            let baseline = self.inline_baseline(key);
            let state = &mut self.fragments.states[key];
            state.baseline = baseline;
            self.fragments.items[state.fragment].layout.baseline = Some(baseline as f64);
        }
    }

    /// Last in-flow line baseline, measured from the border-box top.
    fn last_baseline(&self, key: LayoutInputKey) -> Option<f32> {
        let bx = self.fragments.states[key].layout;

        if let Some(inline) = self.fragments.inline_layouts.get(key) {
            return inline
                .last_baseline
                .map(|baseline| bx.border.top + bx.padding.top + baseline);
        }

        if self.tree.items[key].child_layout != ChildLayout::Block {
            return None;
        }

        self.tree.items.children(key).rev().find_map(|child| {
            let state = self.fragments.states.get(child)?;

            if state.style.position() == taffy::Position::Absolute {
                return None;
            }

            self.last_baseline(child)
                .map(|baseline| state.layout.location.y + baseline)
        })
    }

    /// Baseline relative to the border-box top. The fallback is the bottom
    /// margin edge, which can lie outside the border box.
    fn inline_baseline(&self, key: LayoutInputKey) -> f32 {
        let state = &self.fragments.states[key];

        if state.style.overflow().x == taffy::Overflow::Visible
            && state.style.overflow().y == taffy::Overflow::Visible
            && let Some(baseline) = self.last_baseline(key)
        {
            return baseline;
        }

        state.layout.size.height + state.layout.margin.bottom
    }

    pub fn layout_child(
        &mut self,
        id: taffy::NodeId,
        input: taffy::LayoutInput,
        block_ctx: Option<&mut taffy::BlockContext<'_>>,
    ) -> taffy::LayoutOutput {
        let key = from_taffy_key(id);

        if is_inline_root(self.tree, key) {
            return self.layout_inline(key, input);
        }

        match self.tree.items[key].child_layout {
            ChildLayout::Block => taffy::compute_block_layout(self, id, input, block_ctx),
            ChildLayout::Container => self.layout_container(key, input),
            ChildLayout::None | ChildLayout::Inline => {
                let style = self.fragments.states[key].style.clone();

                taffy::compute_leaf_layout(input, &style, |_, _| 0.0, |_, _| taffy::Size::ZERO)
            }
        }
    }

    /// Lay out a container with its custom algorithm. Containers without an
    /// algorithm are empty leaves.
    fn layout_container(
        &mut self,
        key: LayoutInputKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let ui = self.ui;
        let inner = ui.prop::<DisplayInner>(self.tree.items[key].node);

        let Some(DisplayInner::Container(algorithm)) = inner.as_deref() else {
            let style = self.fragments.states[key].style.clone();

            return taffy::compute_leaf_layout(input, &style, |_, _| 0.0, |_, _| taffy::Size::ZERO);
        };

        let children = self.tree.items.children(key).collect();
        let mut context = ContainerContext {
            engine: self,
            root: key,
            children,
        };

        algorithm.layout(&mut context, input)
    }

    fn layout_inline(
        &mut self,
        key: LayoutInputKey,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let style = self.fragments.states[key].style.clone();
        let padding = style
            .padding()
            .resolve_or_zero(input.parent_size.width, |_, _| 0.0);
        let border = style
            .border()
            .resolve_or_zero(input.parent_size.width, |_, _| 0.0);
        let inset = padding + border;

        let mut output = taffy::compute_leaf_layout(
            input,
            &style,
            |_, _| 0.0,
            |known, available| self.measure_inline(key, known, available),
        );

        if input.run_mode == taffy::RunMode::PerformLayout {
            let width = (output.size.width - inset.left - inset.right).max(0.0);
            let available = taffy::Size {
                width: taffy::AvailableSpace::Definite(width),
                height: input.available_space.height,
            };

            self.prepare_inline(key, available);
            self.break_inline(key, width, true);
            self.emit_inline(
                key,
                taffy::Point {
                    x: inset.left,
                    y: inset.top,
                },
            );
        }

        if let Some(inline) = self.fragments.inline_layouts.get(key) {
            output.baselines = taffy::Baselines {
                first: inline.first_baseline.map(|baseline| inset.top + baseline),
                last: inline.last_baseline.map(|baseline| inset.top + baseline),
            };
        }

        output
    }

    /// Mark the final layout of the item and its ancestors invalid. Measuring
    /// replaces retained lines and descendant geometry, so the final layout
    /// must run again even when its cache entry matches.
    fn invalidate_final(&mut self, key: LayoutInputKey) {
        let mut ancestor = Some(key);

        while let Some(key) = ancestor {
            if let Some(state) = self.fragments.states.get_mut(key) {
                state.final_layout_valid = false;
            }

            if key == self.root {
                break;
            }

            ancestor = self.tree.items.parent(key);
        }
    }

    fn measure_inline(
        &mut self,
        key: LayoutInputKey,
        known: taffy::Size<Option<f32>>,
        available: taffy::Size<taffy::AvailableSpace>,
    ) -> taffy::Size<f32> {
        self.invalidate_final(key);

        let available = taffy::Size {
            width: known
                .width
                .map(taffy::AvailableSpace::Definite)
                .unwrap_or(available.width),
            height: available.height,
        };

        self.prepare_inline(key, available);

        let flow = match available.width {
            taffy::AvailableSpace::Definite(width) => self.break_inline(key, width, false),
            taffy::AvailableSpace::MaxContent => self.break_inline(key, f32::MAX, false),
            taffy::AvailableSpace::MinContent => {
                // Zero width determines the intrinsic minimum. Measure height at
                // that width so short words can share a line in the final layout.
                let flow = self.break_inline(key, 0.0, false);
                self.break_inline(key, flow.width, false)
            }
        };

        taffy::Size {
            width: flow.width,
            height: flow.height,
        }
    }

    /// Shape the inline content if needed, lay out atomic inline and block
    /// children, and update the inline box metrics.
    fn prepare_inline(
        &mut self,
        key: LayoutInputKey,
        available: taffy::Size<taffy::AvailableSpace>,
    ) {
        if !self
            .fragments
            .inline_layouts
            .get(key)
            .is_some_and(|inline| inline.shaped)
        {
            self.shape(key);
        }

        let tree = self.tree;

        for op in InlineEvents::new(tree, key) {
            if let Op::Box(child) | Op::Block(child) = op {
                self.ensure_state(child);
                self.compute(child, available);
            }
        }

        let FragmentTree {
            inline_layouts,
            states,
            ..
        } = &mut *self.fragments;

        for inline_box in inline_layouts[key].layout.inline_boxes_mut() {
            if inline_box.kind != parley::InlineBoxKind::InFlow {
                continue;
            }

            let child = from_box_id(inline_box.id);

            if tree.items[child].kind != LayoutInputKind::AtomicInline {
                continue;
            }

            let state = &states[child];
            let layout = state.layout;

            // Parley places the margin box; fragments retain border-box
            // coordinates and dimensions.
            inline_box.width = layout.size.width + layout.margin.left + layout.margin.right;
            inline_box.height = layout.size.height + layout.margin.top + layout.margin.bottom;
            inline_box.baseline = Some(layout.margin.top + state.baseline);
        }
    }

    /// Build the Parley layout of an inline formatting context. Block children
    /// become custom out-of-flow boxes surrounded by line breaks.
    fn shape(&mut self, key: LayoutInputKey) {
        let tree = self.tree;
        let ui = self.ui;
        let item = &tree.items[key];

        let mut root_style = item.text_style.as_ref().clone();
        // Node identity belongs to the shaping span, not the shared inherited style.
        root_style.brush = Some(item.node);

        let mut builder = self
            .parley
            .tree_builder(self.fonts, 1.0, false, &root_style);

        let span_style = |key: LayoutInputKey| {
            let item = &tree.items[key];
            let mut style = item.text_style.as_ref().clone();
            style.brush = Some(item.node);
            style
        };

        // Inline content on the current line since the last forced break.
        let mut line_open = false;
        // A block was placed and inline content following it needs a new line.
        let mut after_block = false;

        for op in InlineEvents::new(tree, key) {
            match op {
                Op::Push(child) => builder.push_style_span(span_style(child)),
                Op::Pop => builder.pop_style_span(),
                Op::Text(child) => {
                    let Some(text) = ui.prop::<Text>(tree.items[child].node) else {
                        continue;
                    };

                    if text.0.is_empty() {
                        continue;
                    }

                    if std::mem::take(&mut after_block) {
                        builder.push_text("\n");
                    }

                    builder.push_style_span(span_style(child));
                    builder.push_text(&text.0);
                    builder.pop_style_span();
                    line_open = true;
                }
                Op::Box(child) => {
                    if std::mem::take(&mut after_block) {
                        builder.push_text("\n");
                    }

                    // Metrics are set after laying out the box.
                    builder.push_inline_box(parley::InlineBox {
                        id: child.data().as_ffi(),
                        index: 0,
                        width: 0.0,
                        height: 0.0,
                        kind: parley::InlineBoxKind::InFlow,
                        baseline: None,
                        vertical_align: Default::default(),
                    });
                    line_open = true;
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
                Op::Block(child) => {
                    if line_open && !builder.text().ends_with('\n') {
                        builder.push_text("\n");
                    }

                    builder.push_inline_box(parley::InlineBox {
                        id: child.data().as_ffi(),
                        index: 0,
                        width: 0.0,
                        height: 0.0,
                        kind: parley::InlineBoxKind::CustomOutOfFlow,
                        baseline: None,
                        vertical_align: Default::default(),
                    });
                    line_open = false;
                    after_block = true;
                }
            }
        }

        if let Some(inline) = self.fragments.inline_layouts.get_mut(key) {
            inline.text = builder.build_into(&mut inline.layout);
            inline.shaped = true;
        } else {
            let (layout, text) = builder.build();

            self.fragments.inline_layouts.insert(
                key,
                InlineLayout {
                    text,
                    layout,
                    shaped: true,
                    first_baseline: None,
                    last_baseline: None,
                },
            );
        }
    }

    /// Break the inline content into lines of `max_advance`, placing block
    /// children between them. With `emit`, placed content is recorded for
    /// [`Self::emit_inline`].
    fn break_inline(&mut self, key: LayoutInputKey, max_advance: f32, emit: bool) -> Flow {
        let FragmentTree {
            inline_layouts,
            states,
            ..
        } = &mut *self.fragments;
        let inline = &mut inline_layouts[key];
        let events = &mut self.scratch.events;
        events.clear();

        let mut breaker = inline.layout.break_lines();
        breaker.state_mut().set_layout_max_advance(max_advance);
        // Parley 0.12 breaks against the line max advance only. It is never
        // derived from the layout max advance and defaults to zero, so both are set.
        breaker.state_mut().set_line_max_advance(max_advance);

        let mut index = 0;
        let mut width = 0.0f32;
        let mut first_baseline = None;
        let mut last_baseline = None;
        // The current line only holds block boxes and the line break after them.
        let mut block_line = false;

        while let Some(data) = breaker.break_next() {
            match data {
                parley::YieldData::LineBreak(data) => {
                    if std::mem::take(&mut block_line) {
                        // Collapse the line; the blocks already advanced the flow.
                        breaker.state_mut().set_line_y(data.line_y_start);
                    } else {
                        let metrics = data.metrics;
                        width = width.max(metrics.advance - metrics.hanging_advance);
                        first_baseline.get_or_insert(metrics.baseline);
                        last_baseline = Some(metrics.baseline);

                        if emit {
                            events.push(Placed::Line(index));
                        }
                    }

                    index += 1;
                }
                parley::YieldData::InlineBoxBreak(data) => {
                    let child = from_box_id(data.inline_box_id);
                    let layout = states[child].layout;
                    let top = breaker.state().line_y();

                    width = width.max(layout.margin.left + layout.size.width + layout.margin.right);

                    if emit {
                        events.push(Placed::Block {
                            key: child,
                            top: top as f32,
                        });
                    }

                    let height = layout.margin.top + layout.size.height + layout.margin.bottom;
                    let state = breaker.state_mut();
                    state.set_line_y(top + height as f64);
                    // Out-of-flow extents keep the box from contributing to the line.
                    state.append_inline_box_to_line(
                        data.advance,
                        f32::NEG_INFINITY,
                        f32::NEG_INFINITY,
                    );
                    block_line = true;
                }
                // No maximum line height is set.
                parley::YieldData::MaxHeightExceeded(_) => {}
            }
        }

        let height = breaker.committed_y() as f32;
        breaker.finish();

        inline.first_baseline = first_baseline;
        inline.last_baseline = last_baseline;

        Flow { width, height }
    }

    /// Replace the fragments of an inline formatting context with the content
    /// placed by the last [`Self::break_inline`] call. `origin` is the content
    /// box offset from the border box.
    fn emit_inline(&mut self, key: LayoutInputKey, origin: taffy::Point<f32>) {
        let fragment = self.fragments.states[key].fragment;
        self.release_children(fragment);

        let tree = self.tree;
        let Scratch {
            events,
            spans,
            span_parents,
            links,
            ..
        } = &mut *self.scratch;
        let FragmentTree {
            items,
            inline_layouts,
            states,
            first,
            ..
        } = &mut *self.fragments;
        let layout = &inline_layouts[key].layout;

        links.clear();

        for event in events.drain(..) {
            match event {
                Placed::Block { key: child, top } => {
                    let state = &states[child];
                    let location = Point::new(
                        (origin.x + state.layout.location.x) as f64,
                        (origin.y + top + state.layout.margin.top) as f64,
                    );

                    items[state.fragment].layout.location = location;
                    items.append(fragment, state.fragment);
                }
                Placed::Line(index) => {
                    let Some(line) = layout.get(index) else {
                        continue;
                    };
                    let metrics = line.metrics();
                    let left = metrics.inline_min_coord;
                    let top = metrics.block_min_coord;

                    let line_key = items.insert(Fragment::new(
                        key,
                        FragmentItem::Line { line: index },
                        FragmentBox {
                            location: Point::new((origin.x + left) as f64, (origin.y + top) as f64),
                            size: Size::new(
                                (metrics.inline_max_coord - left) as f64,
                                metrics.line_height as f64,
                            ),
                            baseline: Some((metrics.baseline - top) as f64),
                            ..Default::default()
                        },
                    ));
                    items.append(fragment, line_key);
                    span_parents.clear();

                    let mut cx = SpanContext {
                        tree,
                        root: key,
                        line: index,
                        line_key,
                        items,
                        span_parents,
                        spans,
                        links,
                        first,
                    };

                    for (item_index, item) in line.items().enumerate() {
                        match item {
                            parley::PositionedLayoutItem::GlyphRun(run) => {
                                // Line breaks around blocks belong to non-text spans.
                                let Some(source) = run
                                    .style()
                                    .brush
                                    .and_then(|node| tree.item_for_node(node))
                                    .filter(|&source| {
                                        tree.items
                                            .get(source)
                                            .is_some_and(|item| item.kind == LayoutInputKind::Text)
                                    })
                                else {
                                    continue;
                                };

                                let parent = cx.parent(source);
                                let mut layout = FragmentBox {
                                    location: Point::new((run.offset() - left) as f64, 0.0),
                                    size: Size::new(
                                        run.advance() as f64,
                                        metrics.line_height as f64,
                                    ),
                                    ..Default::default()
                                };
                                layout.baseline = Some((run.baseline() - top) as f64);

                                let key = cx.items.insert(Fragment::new(
                                    source,
                                    FragmentItem::GlyphRun {
                                        line: index,
                                        item: item_index,
                                    },
                                    layout,
                                ));
                                cx.items.append(parent, key);
                                cx.link(source, key);
                            }
                            parley::PositionedLayoutItem::InlineBox(bx) => {
                                if bx.kind == parley::InlineBoxKind::CustomOutOfFlow {
                                    continue;
                                }

                                let child = from_box_id(bx.id);
                                let parent = cx.parent(child);

                                if tree.items[child].kind == LayoutInputKind::Inline {
                                    let key = cx.items.insert(Fragment::new(
                                        child,
                                        FragmentItem::Inline { line: index },
                                        FragmentBox {
                                            location: Point::new((bx.x - left) as f64, 0.0),
                                            size: Size::new(0.0, metrics.line_height as f64),
                                            ..Default::default()
                                        },
                                    ));
                                    cx.items.append(parent, key);
                                    cx.link(child, key);

                                    continue;
                                }

                                let state = &states[child];
                                let margin = state.layout.margin;

                                cx.items[state.fragment].layout.location = Point::new(
                                    (bx.x + margin.left - left) as f64,
                                    (bx.y + margin.top - top) as f64,
                                );
                                cx.items.append(parent, state.fragment);
                            }
                        }
                    }
                }
            }
        }

        // Fit inline spans to their contents. Nested spans finish before their parents.
        while let Some(span) = spans.pop() {
            let mut left = f64::INFINITY;
            let mut top = f64::INFINITY;
            let mut right = f64::NEG_INFINITY;
            let mut bottom = f64::NEG_INFINITY;

            for child in items.children(span) {
                let layout = &items[child].layout;
                left = left.min(layout.location.x);
                top = top.min(layout.location.y);
                right = right.max(layout.location.x + layout.size.width);
                bottom = bottom.max(layout.location.y + layout.size.height);
            }

            if left > right {
                continue;
            }

            let layout = &mut items[span].layout;
            layout.location = Point::new(left, top);
            layout.size = Size::new(right - left, bottom - top);

            let mut child = items.first_child(span);
            while let Some(key) = child {
                let layout = &mut items[key].layout;
                layout.location.x -= left;
                layout.location.y -= top;
                child = items.next_sibling(key);
            }
        }

        links.clear();
    }
}

/// Builds the inline span fragments of one line.
struct SpanContext<'a> {
    tree: &'a LayoutInputTree,
    root: LayoutInputKey,
    line: usize,
    line_key: FragmentKey,
    items: &'a mut crate::slot_tree::SlotTree<FragmentKey, Fragment>,
    span_parents: &'a mut FxHashMap<LayoutInputKey, FragmentKey>,
    spans: &'a mut Vec<FragmentKey>,
    links: &'a mut FxHashMap<LayoutInputKey, FragmentKey>,
    first: &'a mut SparseSecondaryMap<LayoutInputKey, FragmentKey>,
}

impl SpanContext<'_> {
    /// The fragment containing `source` on this line, creating span fragments
    /// for its inline ancestors.
    fn parent(&mut self, source: LayoutInputKey) -> FragmentKey {
        // A text root's own runs belong to its line, even if it has a parent.
        if source == self.root {
            return self.line_key;
        }

        let Some(key) = self
            .tree
            .items
            .parent(source)
            .filter(|&key| key != self.root)
        else {
            return self.line_key;
        };

        if let Some(&fragment) = self.span_parents.get(&key) {
            return fragment;
        }

        let parent = self.parent(key);

        if self.tree.items[key].kind != LayoutInputKind::Inline {
            return parent;
        }

        let fragment = self.items.insert(Fragment::new(
            key,
            FragmentItem::Inline { line: self.line },
            FragmentBox::default(),
        ));
        self.items.append(parent, fragment);
        self.link(key, fragment);
        self.spans.push(fragment);
        self.span_parents.insert(key, fragment);

        fragment
    }

    /// Chain fragments generated by the same source across lines.
    fn link(&mut self, source: LayoutInputKey, key: FragmentKey) {
        if let Some(prev) = self.links.insert(source, key) {
            self.items[prev].link.next = Some(key);
            self.items[key].link.prev = Some(prev);
        } else {
            self.first.insert(source, key);
        }
    }
}
