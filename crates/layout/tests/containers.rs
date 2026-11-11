use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use comet_layout::{
    container::{ContainerContext, ContainerLayout},
    fragment::{FragmentItem, FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        LayoutFlags,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint,
        bx::{self, DisplayInner, DisplayOuter},
        geometry::{AutoValue, Size, StyleUnit},
    },
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};
use taffy::style::CoreStyle;

/// Places children in a row of fixed width columns, 7px from the top.
struct Row {
    width: Arc<AtomicU32>,
    calls: Arc<AtomicU32>,
}

impl ContainerLayout for Row {
    fn layout(
        &self,
        context: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        assert!(!context.style().is_block());
        self.calls.fetch_add(1, Ordering::Relaxed);

        let width = self.width.load(Ordering::Relaxed) as f32;
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

fn row(width: &Arc<AtomicU32>, calls: &Arc<AtomicU32>) -> DisplayInner {
    DisplayInner::Container(Box::new(Row {
        width: width.clone(),
        calls: calls.clone(),
    }))
}

#[derive(Default)]
struct Fixture {
    inputs: LayoutInputTree,
    fragments: FragmentTree,
    input_builder: LayoutInputTreeBuilder,
    builder: FragmentTreeBuilder,
}

impl Fixture {
    fn build(&mut self, ui: &Ui, node: NodeKey, width: f64) -> FragmentKey {
        let viewport = Size::new(width, 600.0);
        let root = self
            .input_builder
            .build(
                ui,
                &mut self.inputs,
                BuildRequest::new(node, 16.0, viewport),
            )
            .unwrap();
        let fragment = self
            .builder
            .build(
                ui,
                &self.inputs,
                root,
                &mut self.fragments,
                &LayoutConstraint::new_root(viewport, 16.0),
            )
            .unwrap();
        self.inputs.clear_flags(root);

        fragment
    }

    fn box_fragment(&self, node: NodeKey) -> FragmentKey {
        self.fragments
            .box_fragment(self.inputs.item_for_node(node).unwrap())
            .unwrap()
    }

    fn has_glyphs(&self, key: FragmentKey) -> bool {
        self.fragments.items.descendants(key).any(|key| {
            matches!(
                self.fragments.items[key].item,
                FragmentItem::GlyphRun { .. }
            )
        })
    }
}

#[test]
fn custom_layout_reuses_cache_until_marked_dirty_or_constraints_change() {
    let width = Arc::new(AtomicU32::new(80));
    let calls = Arc::new(AtomicU32::new(0));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let custom = ui.create(Node::Box, (row(&width, &calls),));
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

    let mut f = Fixture::default();

    for (state_width, child_width, dirty, viewport_width, relayout) in [
        (80, 80, false, 400.0, true),
        // Clean trees keep the cached layout, even if external state changed.
        (40, 80, false, 400.0, false),
        (40, 40, true, 400.0, true),
        (100, 100, false, 500.0, true),
    ] {
        width.store(state_width, Ordering::Relaxed);
        calls.store(0, Ordering::Relaxed);

        if dirty {
            f.inputs.mark_node(custom, LayoutFlags::FULL_DIRTY);
        }

        let fragment_root = f.build(&ui, root, viewport_width);
        let custom_fragment = f.box_fragment(custom);
        let children: Vec<_> = f.fragments.items.children(custom_fragment).collect();

        assert_eq!(calls.load(Ordering::Relaxed) > 0, relayout);
        assert_eq!(
            f.fragments.items.parent(custom_fragment),
            Some(fragment_root)
        );
        assert_eq!(children, vec![f.box_fragment(text), f.box_fragment(block)]);
        assert_eq!(f.fragments.items[children[0]].layout.location.x, 0.0);
        assert_eq!(
            f.fragments.items[children[1]].layout.location.x,
            f64::from(child_width)
        );

        for &child in &children {
            let layout = f.fragments.items[child].layout;

            assert_eq!(layout.size.width, f64::from(child_width));
            assert_eq!(layout.location.y, 7.0);
            assert!(layout.size.height > 0.0);
            assert!(f.has_glyphs(child));
        }
    }
}

#[test]
fn nested_inline_containers_measure_and_emit_their_own_descendants() {
    let width = Arc::new(AtomicU32::new(80));
    let calls = Arc::new(AtomicU32::new(0));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, (DisplayOuter::Inline, row(&width, &calls)));
    let inner = ui.create(Node::Box, (row(&width, &calls),));
    let text = ui.create(Node::Text, (Text("nested custom layout".into()),));

    ui.append(root, outer);
    ui.append(outer, inner);
    ui.append(inner, text);

    let mut f = Fixture::default();
    let fragment_root = f.build(&ui, root, 400.0);
    let outer_fragment = f.box_fragment(outer);
    let inner_fragment = f.box_fragment(inner);

    // The inline container is an atomic inline on a line of the root.
    let line = f.fragments.items.parent(outer_fragment).unwrap();
    assert_eq!(f.fragments.items.parent(line), Some(fragment_root));
    assert_eq!(
        f.fragments.items.parent(inner_fragment),
        Some(outer_fragment)
    );

    for fragment in [outer_fragment, inner_fragment] {
        let layout = f.fragments.items[fragment].layout;

        assert_eq!(f.fragments.items[fragment].item, FragmentItem::Box);
        assert!(layout.size.width > 0.0);
        assert!(layout.size.height > 0.0);
    }

    assert!(f.has_glyphs(inner_fragment));
}

#[test]
fn container_children_follow_reorders_and_removals() {
    let width = Arc::new(AtomicU32::new(50));
    let calls = Arc::new(AtomicU32::new(0));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (row(&width, &calls),));
    let children: Vec<_> = ["a", "b", "c"]
        .into_iter()
        .map(|text| {
            let node = ui.create(Node::Text, (Text(text.into()),));
            ui.append(root, node);
            node
        })
        .collect();

    let mut f = Fixture::default();
    let fragment_root = f.build(&ui, root, 400.0);
    let keys: Vec<_> = children.iter().map(|&node| f.box_fragment(node)).collect();

    assert_eq!(
        f.fragments
            .items
            .children(fragment_root)
            .collect::<Vec<_>>(),
        keys
    );

    ui.prepend(root, children[2]);
    ui.delete(children[0]);
    f.inputs.mark_node(root, LayoutFlags::FULL_DIRTY);
    f.build(&ui, root, 400.0);

    // Box fragments of kept children are reused in their new order.
    assert_eq!(
        f.fragments
            .items
            .children(fragment_root)
            .collect::<Vec<_>>(),
        vec![keys[2], keys[1]]
    );
    assert_eq!(f.fragments.items[keys[2]].layout.location.x, 0.0);
    assert_eq!(f.fragments.items[keys[1]].layout.location.x, 50.0);
    assert!(f.fragments.items.get(keys[0]).is_none());
}

/// Probes its only child's size at a width read from `probe`, then lays it
/// out at a fixed 300px.
struct Probe {
    probe: Arc<AtomicU32>,
}

impl ContainerLayout for Probe {
    fn layout(
        &self,
        context: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        let at = |width: f32, run_mode| taffy::LayoutInput {
            run_mode,
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
        };

        let probe = self.probe.load(Ordering::Relaxed) as f32;
        context.measure_child(0, at(probe, taffy::RunMode::ComputeSize));
        let output = context.measure_child(0, at(300.0, input.run_mode));

        if input.run_mode == taffy::RunMode::PerformLayout {
            context.set_child_layout(
                0,
                taffy::Layout {
                    size: output.size,
                    ..Default::default()
                },
            );
        }

        taffy::LayoutOutput::from_outer_size(taffy::Size {
            width: input.known_dimensions.width.unwrap_or(300.0),
            height: input.known_dimensions.height.unwrap_or(output.size.height),
        })
    }
}

#[test]
fn size_probes_do_not_leave_probe_geometry_in_cached_final_layouts() {
    let probe = Arc::new(AtomicU32::new(40));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let custom = ui.create(
        Node::Box,
        (DisplayInner::Container(Box::new(Probe {
            probe: probe.clone(),
        })),),
    );
    // Block container whose size query lays out its children in full.
    let wrapper = ui.create(Node::Box, ());
    let inner = ui.create(Node::Box, ());
    // Stretches to the width `inner` is laid out at.
    let leaf = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Auto,
            AutoValue::Value(StyleUnit::Px(20.0)),
        )),),
    );

    ui.append(root, custom);
    ui.append(custom, wrapper);
    ui.append(wrapper, inner);
    ui.append(inner, leaf);

    let mut f = Fixture::default();
    let width = |f: &Fixture, node| f.fragments.items[f.box_fragment(node)].layout.size.width;

    f.build(&ui, root, 400.0);
    assert_eq!(width(&f, leaf), 300.0);

    // A new probe width misses the wrapper's size cache, while its final
    // layout input is unchanged.
    probe.store(50, Ordering::Relaxed);
    f.inputs.mark_node(custom, LayoutFlags::FULL_DIRTY);
    f.build(&ui, root, 400.0);

    assert_eq!(width(&f, wrapper), 300.0);
    assert_eq!(width(&f, inner), 300.0);
    assert_eq!(width(&f, leaf), 300.0);
}
