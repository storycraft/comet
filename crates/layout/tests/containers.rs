mod common;

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use comet_layout::{
    container::{ContainerContext, ContainerLayout},
    fragment::{FragmentItem, FragmentTree, builder::FragmentBuilder},
    style::{
        LayoutConstraint,
        bx::{DisplayInner, DisplayOuter},
        geometry::Size,
    },
    tree::{
        ChildLayoutMode as Mode, LayoutItemKey, LayoutItemTy as Ty, LayoutTree,
        builder::LayoutTreeBuilder,
    },
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use taffy::style::CoreStyle;

struct Row(Arc<AtomicU32>);

impl ContainerLayout for Row {
    fn layout(
        &self,
        context: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        assert!(!context.style().is_block());

        let width = self.0.load(Ordering::Relaxed) as f32;
        let mut x = 0.0;
        let mut height = 0.0_f32;

        for index in 0..context.child_count() {
            let output = context.measure_child(
                index,
                taffy::LayoutInput {
                    known_dimensions: taffy::Size {
                        width: Some(width),
                        height: None,
                    },
                    available_space: taffy::Size {
                        width: taffy::AvailableSpace::Definite(width),
                        height: taffy::AvailableSpace::MaxContent,
                    },
                    parent_size: taffy::Size {
                        width: Some(width),
                        height: None,
                    },
                    ..input
                },
            );

            if input.run_mode == taffy::RunMode::PerformLayout {
                context.set_child_layout(
                    index,
                    taffy::Layout {
                        size: output.size,
                        location: taffy::Point { x, y: 7.0 },
                        ..Default::default()
                    },
                );
            }

            x += width;
            height = height.max(output.size.height);
        }

        taffy::LayoutOutput::from_outer_size(taffy::Size {
            width: input.known_dimensions.width.unwrap_or(x),
            height: input.known_dimensions.height.unwrap_or(height + 7.0),
        })
    }
}

fn container() -> DisplayInner {
    DisplayInner::Container(Box::new(Row(Arc::new(AtomicU32::new(80)))))
}

fn assert_fresh(ui: &Ui, source: NodeKey, tree: &LayoutTree, key: LayoutItemKey) {
    common::assert_fresh(ui, source, tree, key);

    assert_eq!(tree.items.len(), tree.items.descendants(key).count());
}

#[test]
fn children_have_separate_wrappers_and_blocks_stay_inside_each_child() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (container(),));
    let a = ui.create(Node::Text, ());
    let b = ui.create(Node::Text, ());
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let interruption = ui.create(Node::Box, ());
    let after = ui.create(Node::Text, ());
    let block = ui.create(Node::Box, ());
    let atomic = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let ignored = ui.create(Node::Box, ());

    for child in [a, b, span, block, atomic] {
        ui.append(root, child);
    }

    ui.append(a, ignored);
    ui.append(span, interruption);
    ui.append(span, after);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build(&ui, root, &mut tree, None).unwrap();
    let children: Vec<_> = tree.items.children(key).collect();

    assert_eq!(tree.items[key].child_layout, Mode::Container);
    assert_eq!(children.len(), 5);

    for (index, node) in [(0, a), (1, b), (4, atomic)] {
        assert_eq!(tree.items[children[index]].ty, Ty::AnonymousBlock);
        assert_eq!(tree.items[children[index]].child_layout, Mode::Inline);
        assert_eq!(
            tree.items.parent(tree.item_for_node(node).unwrap()),
            Some(children[index])
        );
    }

    assert!(tree.item_for_node(ignored).is_none());
    assert_eq!(tree.items[children[2]].child_layout, Mode::Block);
    assert_eq!(
        tree.items.parent(tree.item_for_node(interruption).unwrap()),
        Some(children[2])
    );
    assert_eq!(tree.items[children[3]].ty, Ty::Block(block));

    let first = tree.item_for_node(span).unwrap();
    let second = tree.items[first].link.next.unwrap();

    assert_eq!(tree.items[second].link.prev, Some(first));
    assert!(tree.items[second].link.next.is_none());

    let original: Vec<_> = tree.items.descendants(key).collect();

    tree.mark_dirty(key);

    builder.build(&ui, root, &mut tree, Some(key)).unwrap();

    assert_eq!(tree.items.descendants(key).collect::<Vec<_>>(), original);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn inline_container_is_atomic_to_its_surrounding_flow() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (DisplayOuter::Inline,));
    let custom = ui.create(Node::Box, (DisplayOuter::Inline, container()));
    let block = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, ());

    ui.append(root, custom);
    ui.append(custom, block);
    ui.append(root, text);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build(&ui, root, &mut tree, None)
        .unwrap();
    let custom_key = tree.item_for_node(custom).unwrap();

    assert_eq!(tree.items[key].ty, Ty::Inline(root));
    assert_eq!(tree.items[key].child_layout, Mode::Inline);
    assert_eq!(tree.items[custom_key].ty, Ty::AtomicInline(custom));
    assert_eq!(tree.items[custom_key].child_layout, Mode::Container);
    assert_eq!(
        tree.items.parent(tree.item_for_node(block).unwrap()),
        Some(custom_key)
    );
    assert_eq!(tree.items.children(key).count(), 2);
}

#[test]
fn container_flow_transitions_reorders_and_partial_edits_match_fresh_builds() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (container(),));
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let text = ui.create(Node::Text, ());
    let block = ui.create(Node::Box, ());

    ui.append(root, span);
    ui.append(root, text);
    ui.append(span, block);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build(&ui, root, &mut tree, None).unwrap();

    for mode in [
        DisplayInner::Flow,
        container(),
        DisplayInner::FlowRoot,
        container(),
        DisplayInner::Content,
        container(),
    ] {
        ui.set_props(root, (mode,));
        ui.prepend(root, text);
        tree.mark_dirty(key);

        builder.build(&ui, root, &mut tree, Some(key)).unwrap();

        assert_fresh(&ui, root, &tree, key);
    }

    let target = tree.item_for_node(block).unwrap();
    ui.set_props(block, (DisplayOuter::Inline,));
    tree.mark_dirty(target);

    builder.build(&ui, root, &mut tree, Some(key)).unwrap();

    assert_fresh(&ui, root, &tree, key);

    let target = tree.item_for_node(span).unwrap();
    ui.delete(span);

    tree.mark_dirty(target);

    assert_eq!(builder.build(&ui, root, &mut tree, Some(key)), Some(key));
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn custom_layout_reuses_cache_until_invalidated_or_constraints_change() {
    let width = Arc::new(AtomicU32::new(80));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let custom = ui.create(
        Node::Box,
        (DisplayInner::Container(Box::new(Row(width.clone()))),),
    );
    let text = ui.create(
        Node::Text,
        (Text("many words wrapping across several lines".into()),),
    );
    let block = ui.create(Node::Box, ());
    let nested = ui.create(Node::Text, (Text("nested content".into()),));

    ui.append(root, custom);
    ui.append(custom, text);
    ui.append(custom, block);
    ui.append(block, nested);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build(&ui, root, &mut tree, None)
        .unwrap();
    let custom_key = tree.item_for_node(custom).unwrap();
    let mut fragments = FragmentTree::new();
    let mut previous = None;
    let mut builder = FragmentBuilder::new();
    let mut cx = LayoutConstraint::new_root(Size::new(400.0, 600.0), 16.0);

    for (state_width, child_width, invalidate, viewport_width) in [
        (80, 80, false, 400.0),
        (40, 80, false, 400.0),
        (40, 40, true, 400.0),
        (100, 100, false, 500.0),
    ] {
        width.store(state_width, Ordering::Relaxed);
        cx.available_size.width = viewport_width;
        if invalidate {
            fragments.invalidate(key);
        }

        let fragment_root = builder
            .build(&cx, &ui, &tree, key, &mut fragments, previous)
            .unwrap();
        previous = Some(fragment_root);
        let custom_fragment = fragments
            .items
            .descendants(fragment_root)
            .find(|&key| fragments.items[key].source == custom_key)
            .unwrap();
        let children: Vec<_> = fragments.items.children(custom_fragment).collect();

        assert_eq!(children.len(), 2);
        assert_eq!(fragments.items[children[0]].layout.location.x, 0.0);
        assert_eq!(
            fragments.items[children[1]].layout.location.x,
            f64::from(child_width)
        );

        for &child in &children {
            assert_eq!(
                fragments.items[child].layout.size.width,
                f64::from(child_width)
            );
            assert_eq!(fragments.items[child].layout.location.y, 7.0);
            assert!(fragments.items[child].layout.size.height > 0.0);
            assert!(
                fragments
                    .items
                    .descendants(child)
                    .any(|key| matches!(fragments.items[key].item, FragmentItem::GlyphRun { .. }))
            );
        }
    }
}

#[test]
fn nested_inline_containers_measure_and_emit_their_own_descendants() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, (DisplayOuter::Inline, container()));
    let inner = ui.create(Node::Box, (container(),));
    let text = ui.create(Node::Text, (Text("nested custom layout".into()),));

    ui.append(root, outer);
    ui.append(outer, inner);
    ui.append(inner, text);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build(&ui, root, &mut tree, None)
        .unwrap();
    let mut fragments = FragmentTree::new();
    let cx = LayoutConstraint::new_root(Size::new(400.0, 600.0), 16.0);
    let fragment_root = FragmentBuilder::new()
        .build(&cx, &ui, &tree, key, &mut fragments, None)
        .unwrap();

    for node in [outer, inner] {
        let fragment = fragments
            .items
            .descendants(fragment_root)
            .find(|&key| fragments.items[key].node == Some(node))
            .unwrap();

        assert_eq!(fragments.items[fragment].item, FragmentItem::Box);
        assert!(fragments.items[fragment].layout.size.height > 0.0);
        assert!(fragments.items[fragment].layout.size.width > 0.0);
    }

    assert!(fragments.items.descendants(fragment_root).any(|key| {
        fragments.items[key].node == Some(text)
            && matches!(fragments.items[key].item, FragmentItem::GlyphRun { .. })
    }));
}

#[test]
fn containers_receive_original_input_and_return_complete_output() {
    use comet_layout::{
        style::{
            bx,
            geometry::{AutoValue, Rect, StyleUnit},
        },
        taffy::{TaffyLayout, to_taffy_key},
    };
    use taffy::{LayoutPartialTree, TraversePartialTree};

    struct Probe;

    impl ContainerLayout for Probe {
        fn layout(
            &self,
            context: &mut ContainerContext<'_, '_>,
            input: taffy::LayoutInput,
        ) -> taffy::LayoutOutput {
            assert_eq!(input, probe_input(input.run_mode));

            context.set_child_layout(
                0,
                taffy::Layout {
                    location: taffy::Point { x: 3.0, y: 7.0 },
                    ..Default::default()
                },
            );

            probe_output()
        }
    }

    fn probe_input(run_mode: taffy::RunMode) -> taffy::LayoutInput {
        taffy::LayoutInput {
            run_mode,
            known_dimensions: taffy::Size {
                width: Some(120.0),
                height: None,
            },
            known_dimensions_are_definite: taffy::Size {
                width: false,
                height: true,
            },
            parent_size: taffy::Size {
                width: Some(400.0),
                height: Some(600.0),
            },
            available_space: taffy::Size {
                width: taffy::AvailableSpace::Definite(200.0),
                height: taffy::AvailableSpace::MinContent,
            },
            vertical_margins_are_collapsible: taffy::Line {
                start: true,
                end: false,
            },
            ..taffy::LayoutInput::HIDDEN
        }
    }

    fn probe_output() -> taffy::LayoutOutput {
        let mut output = taffy::LayoutOutput::from_outer_size(taffy::Size {
            width: 120.0,
            height: 80.0,
        });
        output.baselines = taffy::Baselines {
            first: Some(17.0),
            last: Some(37.0),
        };

        output
    }

    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            DisplayInner::Container(Box::new(Probe)),
            bx::Size(Size::new(
                AutoValue::Value(StyleUnit::Px(120.0)),
                AutoValue::Value(StyleUnit::Px(80.0)),
            )),
            bx::Padding(Rect::new_invidiual(
                StyleUnit::Px(10.0),
                StyleUnit::Px(10.0),
                StyleUnit::Px(10.0),
                StyleUnit::Px(10.0),
            )),
            bx::Border(Rect::new_invidiual(
                StyleUnit::Px(2.0),
                StyleUnit::Px(2.0),
                StyleUnit::Px(2.0),
                StyleUnit::Px(2.0),
            )),
        ),
    );
    let child = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, (Text("child content".into()),));

    ui.append(root, child);
    ui.append(child, text);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build(&ui, root, &mut tree, None)
        .unwrap();
    let child_key = tree.item_for_node(child).unwrap();
    let mut fragments = FragmentTree::new();
    let mut fonts = parley::FontContext::new();
    let mut parley = parley::LayoutContext::new();
    let cx = LayoutConstraint::new_root(Size::new(400.0, 600.0), 16.0);
    let mut engine = TaffyLayout::new(
        &ui,
        &tree,
        &mut fragments,
        &mut fonts,
        &mut parley,
        &cx,
        key,
    )
    .unwrap();

    assert_eq!(tree.items.children(key).count(), 1);
    assert_eq!(engine.child_count(to_taffy_key(key)), 0);
    assert_eq!(engine.child_ids(to_taffy_key(key)).count(), 0);

    for mode in [taffy::RunMode::ComputeSize, taffy::RunMode::PerformLayout] {
        let output = engine.compute_child_layout(to_taffy_key(key), probe_input(mode));

        assert_eq!(output, probe_output());
        assert_eq!(
            engine.layout(child_key).unwrap().location,
            taffy::Point { x: 3.0, y: 7.0 }
        );
    }
}

struct Gap(f32);
impl comet_tree::style::StyleProp for Gap {}

struct ChildWidth(f32);
impl comet_tree::style::StyleProp for ChildWidth {}

struct StyledRow;

impl ContainerLayout for StyledRow {
    fn layout(
        &self,
        context: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let gap = context.prop::<Gap>().map_or(0.0, |gap| gap.0);
        let mut x = 0.0;
        let mut height = 0.0_f32;

        assert!(context.node().is_some());

        for index in 0..context.child_count() {
            let width = context
                .child_prop::<ChildWidth>(index)
                .map_or(10.0, |width| width.0);

            assert!(context.child_node(index).is_some());
            assert!(context.child_prop::<Gap>(index).is_none());

            let output = context.measure_child(
                index,
                taffy::LayoutInput {
                    known_dimensions: taffy::Size {
                        width: Some(width),
                        height: None,
                    },
                    ..input
                },
            );

            if input.run_mode == taffy::RunMode::PerformLayout {
                context.set_child_layout(
                    index,
                    taffy::Layout {
                        size: output.size,
                        location: taffy::Point { x, y: 0.0 },
                        ..Default::default()
                    },
                );
            }

            x += width + gap;
            height = height.max(output.size.height);
        }

        taffy::LayoutOutput::from_outer_size(taffy::Size { width: x, height })
    }
}

#[test]
fn custom_props_resolve_wrapped_sources_and_invalidate_cached_layout() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            DisplayInner::Container(Box::new(StyledRow)),
            Gap(3.0),
            ChildWidth(999.0),
        ),
    );
    let text = ui.create(Node::Text, (ChildWidth(21.0),));
    let span = ui.create(Node::Box, (DisplayOuter::Inline, ChildWidth(32.0)));
    let interruption = ui.create(Node::Box, (ChildWidth(777.0),));
    let atomic = ui.create(
        Node::Box,
        (
            DisplayOuter::Inline,
            DisplayInner::FlowRoot,
            ChildWidth(43.0),
        ),
    );
    let block = ui.create(Node::Box, (ChildWidth(54.0),));
    let default = ui.create(Node::Text, ());

    for child in [text, span, atomic, block, default] {
        ui.append(root, child);
    }

    ui.append(span, interruption);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let key = builder.build(&ui, root, &mut tree, None).unwrap();
    let mut fragments = FragmentTree::new();
    let mut previous = None;
    let mut fragment_builder = FragmentBuilder::new();
    let cx = LayoutConstraint::new_root(Size::new(500.0, 600.0), 16.0);

    for (gap, widths) in [
        (3.0, [21.0, 32.0, 43.0, 54.0, 10.0]),
        (8.0, [21.0, 62.0, 43.0, 54.0, 10.0]),
    ] {
        ui.set_props(root, (Gap(gap),));
        ui.set_props(span, (ChildWidth(widths[1]),));
        fragments.invalidate(key);

        let result = fragment_builder
            .build(&cx, &ui, &tree, key, &mut fragments, previous)
            .unwrap();
        previous = Some(result);
        let children: Vec<_> = fragments.items.children(result).collect();
        let mut x = 0.0_f32;

        assert_eq!(children.len(), widths.len());

        for (&child, width) in children.iter().zip(widths) {
            assert_eq!(fragments.items[child].layout.size.width, f64::from(width));
            assert_eq!(fragments.items[child].layout.location.x, f64::from(x));

            x += width + gap;
        }
    }
}
