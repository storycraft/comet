use comet_tree::{
    node::{NodeKey, Text},
    ui::Ui,
};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::KeyData;

use super::{Fragment, FragmentBox, FragmentItem, FragmentKey, FragmentLink, tree::FragmentTree};
use crate::{
    style::{
        LayoutConstraint,
        geometry::{Point, Size},
    },
    taffy::{NodeState, TaffyLayout, conv, style},
    tree::{ChildLayoutMode, LayoutItem, LayoutItemKey, LayoutTree},
};

#[derive(PartialEq)]
pub(crate) struct Snapshot {
    constraint: LayoutConstraint,
    inputs: Vec<InputSnapshot>,
}

#[derive(PartialEq)]
struct InputSnapshot {
    key: LayoutItemKey,
    item: LayoutItem,
    style: taffy::Style,
    text_style: Option<super::TextStyle>,
    text: Option<String>,
    children: Vec<LayoutItemKey>,
}

/// Reconciles a layout subtree into stable fragment slots. Unchanged inputs skip
/// shaping and layout; changed inputs reuse matching box, line, and run slots.
#[derive(Default)]
pub struct FragmentBuilder {
    pub fonts: parley::FontContext,
    parley: parley::LayoutContext<Option<NodeKey>>,
}

impl FragmentBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Call after updating `LayoutTree`. `root` identifies an independent layout root.
    pub fn build(
        &mut self,
        constraint: &LayoutConstraint,
        ui: &Ui,
        layout: &LayoutTree,
        root: LayoutItemKey,
        fragments: &mut FragmentTree,
    ) -> Option<FragmentKey> {
        let old_root = fragments.root_for_item(root);
        let mut old = Vec::new();
        if let Some(key) = old_root {
            descendants(fragments, key, &mut old);
        }

        if layout.items.get(root).is_none() {
            let sources: Vec<_> = fragments
                .snapshots
                .get(&root)
                .into_iter()
                .flat_map(|snapshot| snapshot.inputs.iter().map(|input| input.key))
                .collect();
            for key in sources {
                fragments.inline_layouts.remove(&key);
                fragments.texts.remove(&key);
                fragments.layout_cache.remove(&key);
            }
            if let Some(key) = old_root {
                fragments.items.delete_node(key);
            }
            fragments.roots.remove(&root);
            fragments.snapshots.remove(&root);

            return None;
        }

        let mut pending = vec![root];
        let mut keys = Vec::new();
        let mut nodes = FxHashMap::default();
        let mut resolved = FxHashMap::default();
        let mut snapshot = Snapshot {
            constraint: constraint.clone(),
            inputs: Vec::new(),
        };

        while let Some(key) = pending.pop() {
            let item = layout.items[key];
            let mut cx = constraint.clone();
            let text_style =
                crate::parley::style::resolve_for_item(ui, layout, key, constraint, &mut resolved);
            if let Some(style) = &text_style {
                cx.font_size = style.font_size as f64;
            }

            let style = style::resolve(ui, item.ty.node(), &cx);
            let children: Vec<_> = layout.items.children(key).collect();
            let text = item
                .ty
                .node()
                .and_then(|node| ui.prop::<Text>(node))
                .map(|text| text.0.clone());
            snapshot.inputs.push(InputSnapshot {
                key,
                item,
                style,
                text_style,
                text,
                children: children.clone(),
            });
            keys.push(key);
            pending.extend(children.into_iter().rev());
        }

        fragments.text_styles.extend(resolved);
        if old_root.is_some() && fragments.snapshots.get(&root) == Some(&snapshot) {
            return old_root;
        }

        let previous = fragments.snapshots.get(&root);
        let previous_keys: Vec<_> = previous
            .into_iter()
            .flat_map(|snapshot| snapshot.inputs.iter().map(|input| input.key))
            .collect();
        let previous_inputs: FxHashMap<_, _> = previous
            .into_iter()
            .flat_map(|snapshot| &snapshot.inputs)
            .map(|input| (input.key, input))
            .collect();
        let constraint_changed = previous.is_none_or(|snapshot| snapshot.constraint != *constraint);
        let mut dirty = FxHashSet::default();

        for input in &snapshot.inputs {
            if constraint_changed || previous_inputs.get(&input.key).copied() != Some(input) {
                let mut ancestor = Some(input.key);
                while let Some(key) = ancestor {
                    if !dirty.insert(key) {
                        break;
                    }
                    if key == root {
                        break;
                    }
                    ancestor = layout.items.parent(key);
                }
            }
        }

        for input in &snapshot.inputs {
            let mut state = fragments
                .layout_cache
                .remove(&input.key)
                .unwrap_or_else(|| NodeState {
                    style: input.style.clone(),
                    layout: Default::default(),
                    cache: Default::default(),
                    hoisted: Vec::new(),
                });
            state.style = input.style.clone();
            if dirty.contains(&input.key) {
                state.cache.clear();
                fragments.inline_layouts.remove(&input.key);
                fragments.texts.remove(&input.key);
            }
            nodes.insert(input.key, state);
        }

        let mut engine = TaffyLayout {
            ui,
            tree: layout,
            fragments,
            fonts: &mut self.fonts,
            parley: &mut self.parley,
            nodes,
        };
        engine.compute(
            root,
            conv::to_size(constraint.available_size, |v| {
                taffy::AvailableSpace::Definite(v as f32)
            }),
        );

        let mut desired = Vec::new();
        emit_box(&engine, root, None, None, &mut desired);
        engine.fragments.layout_cache.extend(engine.nodes);
        let fragments = engine.fragments;
        let mut reusable: FxHashMap<_, Vec<FragmentKey>> = FxHashMap::default();
        let mut old_sources = FxHashSet::default();
        old_sources.extend(previous_keys);

        for key in &old {
            let f = &fragments.items[*key];
            old_sources.insert(f.source);
            reusable
                .entry((f.source, f.node, f.item.clone()))
                .or_default()
                .push(*key);
        }

        // Detach before deleting stale parents so reusable descendants survive.
        for key in &old {
            fragments.items.remove_parent(*key);
        }

        let mut result = Vec::new();
        let mut used = FxHashSet::default();
        let mut last: FxHashMap<NodeKey, FragmentKey> = FxHashMap::default();
        for (parent, fragment) in desired {
            let identity = (fragment.source, fragment.node, fragment.item.clone());
            let key = reusable
                .get_mut(&identity)
                .and_then(|keys| keys.pop())
                .unwrap_or_else(|| fragments.items.insert(fragment.clone()));
            fragments.items[key] = fragment;
            used.insert(key);
            if let Some(parent) = parent {
                fragments.items.append(result[parent], key);
            }

            if !matches!(fragments.items[key].item, FragmentItem::Line { .. })
                && let Some(node) = fragments.items[key].node
                && let Some(prev) = last.insert(node, key)
            {
                fragments.items[prev].link.next = Some(key);
                fragments.items[key].link.prev = Some(prev);
            }

            result.push(key);
        }

        for key in old {
            if !used.contains(&key) {
                fragments.items.delete_node(key);
            }
        }
        for key in old_sources {
            if !keys.contains(&key) {
                fragments.inline_layouts.remove(&key);
                fragments.texts.remove(&key);
                fragments.layout_cache.remove(&key);
            }
        }

        let result = result[0];
        fragments.roots.insert(root, result);
        fragments.snapshots.insert(root, snapshot);
        fragments
            .text_styles
            .retain(|node, _| ui.node(*node).is_some());

        Some(result)
    }
}

fn descendants(tree: &FragmentTree, key: FragmentKey, output: &mut Vec<FragmentKey>) {
    output.push(key);
    for child in tree.items.children(key) {
        descendants(tree, child, output);
    }
}

type Desired = Vec<(Option<usize>, Fragment)>;

fn fragment(
    source: LayoutItemKey,
    node: Option<NodeKey>,
    item: FragmentItem,
    location: Point<f64>,
    size: Size<f64>,
) -> Fragment {
    Fragment {
        source,
        node,
        item,
        layout: FragmentBox {
            location,
            size,
            ..Default::default()
        },
        link: FragmentLink::default(),
    }
}

fn emit_box(
    engine: &TaffyLayout<'_>,
    key: LayoutItemKey,
    parent: Option<usize>,
    position: Option<Point<f64>>,
    out: &mut Desired,
) {
    let bx = engine.nodes[&key].layout;
    let index = out.len();
    let mut f = fragment(
        key,
        engine.tree.items[key].ty.node(),
        FragmentItem::Box,
        position.unwrap_or_else(|| conv::from_point(bx.location, f64::from)),
        conv::from_size(bx.size, f64::from),
    );
    f.layout.padding = conv::from_rect(bx.padding, f64::from);
    f.layout.border = conv::from_rect(bx.border, f64::from);
    if matches!(
        engine.tree.items[key].ty,
        crate::tree::LayoutItemTy::AtomicInline(_)
    ) {
        f.layout.baseline = Some(engine.inline_baseline(key) as f64);
    }
    out.push((parent, f));

    if engine.tree.items[key].child_layout == ChildLayoutMode::Block {
        for child in engine.tree.items.children(key) {
            emit_box(engine, child, Some(index), None, out);
        }
    }

    let Some(layout) = engine.fragments.inline_layouts.get(&key) else {
        return;
    };
    for (line_index, line) in layout.lines().enumerate() {
        let metrics = line.metrics();
        let line_parent = out.len();
        let mut line_fragment = fragment(
            key,
            None,
            FragmentItem::Line { line: line_index },
            Point::new(
                (bx.border.left + bx.padding.left + metrics.inline_min_coord) as f64,
                (bx.border.top + bx.padding.top + metrics.block_min_coord) as f64,
            ),
            Size::new(
                (metrics.inline_max_coord - metrics.inline_min_coord) as f64,
                metrics.line_height as f64,
            ),
        );
        line_fragment.layout.baseline = Some((metrics.baseline - metrics.block_min_coord) as f64);
        out.push((Some(index), line_fragment));
        let mut spans = Vec::new();
        let mut span_indices = FxHashMap::default();

        for (item_index, item) in line.items().enumerate() {
            match item {
                parley::PositionedLayoutItem::GlyphRun(run) => {
                    let node = run.style().brush;
                    let source = node
                        .and_then(|node| find_source(engine.tree, key, node))
                        .unwrap_or(key);
                    let parent = inline_parent(
                        engine.tree,
                        key,
                        source,
                        line_index,
                        line_parent,
                        &mut spans,
                        &mut span_indices,
                        out,
                    );
                    let mut f = fragment(
                        source,
                        node,
                        FragmentItem::GlyphRun {
                            line: line_index,
                            item: item_index,
                        },
                        Point::new((run.offset() - metrics.inline_min_coord) as f64, 0.0),
                        Size::new(run.advance() as f64, metrics.line_height as f64),
                    );
                    f.layout.baseline = Some((run.baseline() - metrics.block_min_coord) as f64);
                    out.push((Some(parent), f));
                }
                parley::PositionedLayoutItem::InlineBox(bx) => {
                    let child = LayoutItemKey::from(KeyData::from_ffi(bx.id));
                    let parent = inline_parent(
                        engine.tree,
                        key,
                        child,
                        line_index,
                        line_parent,
                        &mut spans,
                        &mut span_indices,
                        out,
                    );
                    if matches!(
                        engine.tree.items[child].ty,
                        crate::tree::LayoutItemTy::Inline(_)
                    ) {
                        out.push((
                            Some(parent),
                            fragment(
                                child,
                                engine.tree.items[child].ty.node(),
                                FragmentItem::Inline { line: line_index },
                                Point::new((bx.x - metrics.inline_min_coord) as f64, 0.0),
                                Size::new(0.0, metrics.line_height as f64),
                            ),
                        ));

                        continue;
                    }

                    emit_box(
                        engine,
                        child,
                        Some(parent),
                        Some(Point::new(
                            (bx.x - metrics.inline_min_coord) as f64,
                            (bx.y - metrics.block_min_coord) as f64,
                        )),
                        out,
                    );
                }
            }
        }

        for span in spans.into_iter().rev() {
            let children: Vec<_> = (span + 1..out.len())
                .filter(|&i| out[i].0 == Some(span))
                .collect();
            let mut left = f64::INFINITY;
            let mut top = f64::INFINITY;
            let mut right = f64::NEG_INFINITY;
            let mut bottom = f64::NEG_INFINITY;
            for &child in &children {
                let f = &out[child].1;
                left = left.min(f.layout.location.x);
                top = top.min(f.layout.location.y);
                right = right.max(f.layout.location.x + f.layout.size.width);
                bottom = bottom.max(f.layout.location.y + f.layout.size.height);
            }

            if children.is_empty() {
                continue;
            }
            out[span].1.layout.location = Point::new(left, top);
            out[span].1.layout.size = Size::new(right - left, bottom - top);
            for child in children {
                out[child].1.layout.location.x -= left;
                out[child].1.layout.location.y -= top;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn inline_parent(
    tree: &LayoutTree,
    root: LayoutItemKey,
    source: LayoutItemKey,
    line: usize,
    line_parent: usize,
    spans: &mut Vec<usize>,
    indices: &mut FxHashMap<LayoutItemKey, usize>,
    out: &mut Desired,
) -> usize {
    let mut chain = Vec::new();
    let mut ancestor = tree.items.parent(source);
    while let Some(key) = ancestor {
        if key == root {
            break;
        }
        if matches!(tree.items[key].ty, crate::tree::LayoutItemTy::Inline(_)) {
            chain.push(key);
        }
        ancestor = tree.items.parent(key);
    }

    let mut parent = line_parent;
    for key in chain.into_iter().rev() {
        parent = *indices.entry(key).or_insert_with(|| {
            let index = out.len();
            out.push((
                Some(parent),
                fragment(
                    key,
                    tree.items[key].ty.node(),
                    FragmentItem::Inline { line },
                    Point::default(),
                    Size::default(),
                ),
            ));
            spans.push(index);

            index
        });
    }

    parent
}

fn find_source(tree: &LayoutTree, root: LayoutItemKey, node: NodeKey) -> Option<LayoutItemKey> {
    if tree.items[root].ty.node() == Some(node) {
        return Some(root);
    }

    tree.items
        .children(root)
        .find_map(|child| find_source(tree, child, node))
}
