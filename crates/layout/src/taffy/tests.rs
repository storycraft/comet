use comet_tree::node::Node;
use taffy::{AvailableSpace, LayoutInput, LayoutPartialTree, RunMode};

use super::*;
use crate::{
    style::{
        LayoutConstraint, bx,
        geometry::{Rect, Size, StyleUnit},
    },
    tree::builder::LayoutTreeBuilder,
};

fn text_metrics(text: &parley::Layout<Option<NodeKey>>) -> (f32, f32, usize, Option<f32>, f32) {
    (
        text.width(),
        text.height(),
        text.len(),
        text.last_baseline(),
        text.layout_max_advance(),
    )
}

#[test]
fn intrinsic_text_height_matches_retained_lines() {
    let mut fonts = parley::FontContext::new();
    let mut parley = parley::LayoutContext::new();

    for width in [
        AvailableSpace::MaxContent,
        AvailableSpace::MinContent,
        AvailableSpace::Definite(90.0),
    ] {
        for padding in [0.0, 0.1, 0.3, 1.1, 10.1] {
            for text in [
                "one two three four five",
                "hello world",
                "a long sentence with many words to wrap",
            ] {
                let mut ui = Ui::new();
                let root = ui.create(
                    Node::Box,
                    (
                        bx::DisplayInner::FlowRoot,
                        bx::Padding(Rect::new_axis(
                            StyleUnit::Px(padding),
                            StyleUnit::Px(padding),
                        )),
                    ),
                );
                let child = ui.create(Node::Text, (Text(text.into()),));
                ui.append(root, child);

                let mut tree = LayoutTree::new();
                let key = LayoutTreeBuilder::new()
                    .build(&ui, root, &mut tree, None)
                    .unwrap();
                let mut fragments = FragmentTree::new();
                let constraint = LayoutConstraint::new_root(Size::new(500.0, 600.0), 16.0);
                let mut engine = TaffyLayout::new(
                    &ui,
                    &tree,
                    &mut fragments,
                    &mut fonts,
                    &mut parley,
                    &constraint,
                    key,
                )
                .unwrap();

                engine.compute(
                    key,
                    taffy::Size {
                        width,
                        height: AvailableSpace::MaxContent,
                    },
                );

                let bx = engine.layout(key).unwrap();
                let text = &engine.fragments.inline_layouts[key].layout;
                let content_height = bx.size.height - bx.padding.top - bx.padding.bottom;
                assert!((content_height - text.height()).abs() < 0.01);

                if width == AvailableSpace::MaxContent {
                    assert_eq!(text.len(), 1);
                }

                if width == AvailableSpace::MinContent {
                    let mut at_minimum = text.clone();
                    at_minimum.break_all_lines(Some(text.width()));
                    assert_eq!(at_minimum.height(), text.height());
                }
            }
        }
    }
}

#[test]
fn final_layout_after_sizing_probes_matches_fresh_layout() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (bx::DisplayInner::FlowRoot,));
    let before = ui.create(Node::Text, (Text("before the inline box ".into()),));
    let atomic = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    let text = ui.create(
        Node::Text,
        (Text("a long sentence with many words to wrap".into()),),
    );
    ui.append(root, before);
    ui.append(root, atomic);
    ui.append(atomic, text);

    let mut tree = LayoutTree::new();
    let key = LayoutTreeBuilder::new()
        .build(&ui, root, &mut tree, None)
        .unwrap();
    let mut fragments = FragmentTree::new();
    let mut fonts = parley::FontContext::new();
    let mut parley = parley::LayoutContext::new();
    let constraint = LayoutConstraint::new_root(Size::new(500.0, 600.0), 16.0);
    let mut engine = TaffyLayout::new(
        &ui,
        &tree,
        &mut fragments,
        &mut fonts,
        &mut parley,
        &constraint,
        key,
    )
    .unwrap();
    let available = taffy::Size {
        width: AvailableSpace::Definite(120.0),
        height: AvailableSpace::MaxContent,
    };

    engine.compute(key, available);

    let layouts: FxHashMap<_, _> = engine
        .nodes
        .iter()
        .map(|(key, state)| (key, state.layout))
        .collect();
    let texts: FxHashMap<_, _> = engine
        .fragments
        .inline_layouts
        .iter()
        .map(|(key, text)| (key, text_metrics(&text.layout)))
        .collect();

    assert_eq!(texts.len(), 2);

    for width in [
        AvailableSpace::MinContent,
        AvailableSpace::MaxContent,
        AvailableSpace::Definite(60.0),
    ] {
        let input = LayoutInput {
            run_mode: RunMode::ComputeSize,
            available_space: taffy::Size {
                width,
                height: AvailableSpace::MaxContent,
            },
            ..LayoutInput::HIDDEN
        };

        let atomic_key = tree.item_for_node(atomic).unwrap();

        for probe in [key, key, atomic_key, atomic_key] {
            engine.compute_child_layout(to_taffy_key(probe), input);

            // Sizing may replace intermediate text and geometry. Only the
            // subsequent final layout must match a fresh layout of the tree.
            engine.compute(key, available);

            for (&key, expected) in &layouts {
                assert_eq!(&engine.nodes[key].layout, expected);
            }

            for (&key, expected) in &texts {
                let text = &engine.fragments.inline_layouts[key].layout;
                assert_eq!(&text_metrics(text), expected);
            }
        }
    }
}
