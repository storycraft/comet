use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use comet_layout::{
    container::{ContainerContext, ContainerLayout},
    fragment::{
        FragmentItem, FragmentKey, builder::FragmentTreeBuilder, state::NodeLayoutBits,
        tree::FragmentTree,
    },
    input::{
        InputFlags,
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
    ) -> (taffy::LayoutOutput, NodeLayoutBits) {
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

        let output = taffy::LayoutOutput::from_outer_size(taffy::Size {
            width: input.known_dimensions.width.unwrap_or(x),
            height: input.known_dimensions.height.unwrap_or(height + 7.0),
        });

        (output, NodeLayoutBits::empty())
    }
}

fn row(width: &Arc<AtomicU32>, calls: &Arc<AtomicU32>) -> DisplayInner {
    DisplayInner::Container(Arc::new(Row {
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
fn custom_layout_runs_again_when_its_box_is_dirty() {
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
        // Without marking or a viewport change, boxes are kept, so external
        // state is not picked up.
        (40, 80, false, 400.0, false),
        (40, 40, true, 400.0, true),
        // A viewport change lays out the boxes it resizes again.
        (100, 100, false, 500.0, true),
    ] {
        width.store(state_width, Ordering::Relaxed);
        calls.store(0, Ordering::Relaxed);

        if dirty {
            f.inputs.mark_node(custom, InputFlags::NEED_FULL_LAYOUT);
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

        assert!(f.fragments.items[fragment].item.is_box());
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
    f.inputs.mark_node(root, InputFlags::NEED_CHILDREN);
    f.build(&ui, root, 400.0);

    // Box fragments of kept children are reused in their new order. The
    // moved child gets a new box fragment.
    let moved = f.box_fragment(children[2]);
    assert_ne!(moved, keys[2]);
    assert_eq!(
        f.fragments
            .items
            .children(fragment_root)
            .collect::<Vec<_>>(),
        vec![moved, keys[1]]
    );
    assert_eq!(f.fragments.items[moved].layout.location.x, 0.0);
    assert_eq!(f.fragments.items[keys[1]].layout.location.x, 50.0);
    assert!(f.fragments.items.get(keys[0]).is_none());
    assert!(f.fragments.items.get(keys[2]).is_none());
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
    ) -> (taffy::LayoutOutput, NodeLayoutBits) {
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

        let output = taffy::LayoutOutput::from_outer_size(taffy::Size {
            width: input.known_dimensions.width.unwrap_or(300.0),
            height: input.known_dimensions.height.unwrap_or(output.size.height),
        });

        (output, NodeLayoutBits::empty())
    }
}

#[test]
fn size_probes_do_not_leave_probe_geometry_in_cached_final_layouts() {
    let probe = Arc::new(AtomicU32::new(40));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let custom = ui.create(
        Node::Box,
        (DisplayInner::Container(Arc::new(Probe {
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
    f.inputs.mark_node(custom, InputFlags::NEED_FULL_LAYOUT);
    f.build(&ui, root, 400.0);

    assert_eq!(width(&f, wrapper), 300.0);
    assert_eq!(width(&f, inner), 300.0);
    assert_eq!(width(&f, leaf), 300.0);
}

/// The fragments under `key` in tree order, with their depth, kind and size.
fn outline(f: &Fixture, key: FragmentKey, depth: usize, out: &mut Vec<String>) {
    let fragment = &f.fragments.items[key];
    let kind = match fragment.item {
        FragmentItem::Box { .. } => "box",
        FragmentItem::AnonymousBlock { .. } => "anonymous",
        FragmentItem::Line { .. } => "line",
        FragmentItem::GlyphRun { .. } => "glyphs",
        FragmentItem::Inline { .. } => "inline",
    };
    out.push(format!("{depth} {kind} {}", fragment.layout.size));

    for child in f.fragments.items.children(key) {
        outline(f, child, depth + 1, out);
    }
}

fn outlined(f: &Fixture, key: FragmentKey) -> Vec<String> {
    let mut out = Vec::new();
    outline(f, key, 0, &mut out);
    out
}

#[test]
fn children_gaining_or_losing_boxes_with_their_parents_layout_match_a_fresh_build() {
    let width = Arc::new(AtomicU32::new(100));
    let calls = Arc::new(AtomicU32::new(0));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    // An inline element only generates a box as a child of a container.
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let inner = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, (Text("inside".into()),));
    ui.append(root, span);
    ui.append(span, inner);
    ui.append(inner, text);

    let mut f = Fixture::default();
    f.build(&ui, root, 400.0);

    let check = |f: &mut Fixture, ui: &Ui| {
        f.inputs.mark_node(root, InputFlags::NEED_FULL_LAYOUT);
        let fragment = f.build(ui, root, 400.0);

        let mut fresh = Fixture::default();
        let fresh_fragment = fresh.build(ui, root, 400.0);
        assert_eq!(outlined(f, fragment), outlined(&fresh, fresh_fragment));
    };

    // The span gains a box, holding its own content.
    ui.set_props(root, (row(&width, &calls),));
    check(&mut f, &ui);
    assert!(f.has_glyphs(f.box_fragment(span)));

    // And loses it again, its content joining the root's.
    drop(ui.remove_prop::<DisplayInner>(root));
    check(&mut f, &ui);
    assert!(
        f.fragments
            .box_fragment(f.inputs.item_for_node(span).unwrap())
            .is_none()
    );
}
