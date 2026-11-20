use std::{cell::RefCell, rc::Rc};

use any_spawner::Executor;
use anyrender::Paint;
use comet_layout::{
    dirty_map::DirtyFlags,
    fragment::{builder::FragmentTreeBuilder, tree::FragmentTree},
    input::builder::{BuildRequest, LayoutInputTreeBuilder},
    style::{
        LayoutConstraint,
        bx::{DisplayInner, DisplayOuter, FontSize, MinSize, Padding},
        geometry::{AutoValue, Rect, Size, StyleUnit},
    },
};
use comet_layout_flex::styles::Gap;
use comet_paint::style::TextColor;
use comet_reactivity::{
    UiCx, UiTree,
    prelude::{FlexStyled, Styled, auto, axis, percent, px, vh},
};
use comet_tree::node::{Node, NodeKey, Text};
use peniko::Color;
use reactive_graph::{
    owner::Owner,
    signal::{ReadSignal, signal},
    traits::{Get, Set},
};
use tokio::task::LocalSet;

/// Run `test` on a local set with the tokio executor, under a root owner.
async fn run(test: impl AsyncFnOnce()) {
    // Only the first test of the process initializes it.
    let _ = Executor::init_tokio();
    let owner = Owner::new();
    owner.set();

    LocalSet::new().run_until(test()).await;
    owner.unset();
}

/// Let pending bindings run.
async fn settle() {
    for _ in 0..4 {
        Executor::tick().await;
        tokio::task::yield_now().await;
    }
}

fn shared() -> Rc<RefCell<UiTree>> {
    Rc::new(RefCell::new(UiTree::new()))
}

fn layout(tree: &mut UiTree, root: NodeKey, fragments: &mut FragmentTree) {
    let viewport = Size::new(800.0, 600.0);
    let UiTree {
        ui, inputs, dirty, ..
    } = tree;

    let input_root = LayoutInputTreeBuilder::new()
        .build(ui, inputs, dirty, BuildRequest::new(root, 16.0, viewport))
        .expect("input tree");
    FragmentTreeBuilder::new()
        .build(
            ui,
            inputs,
            dirty,
            input_root,
            fragments,
            &LayoutConstraint::new_root(viewport, 16.0),
        )
        .expect("fragment tree");
    dirty.clear(inputs, input_root);
}

fn flags(tree: &UiTree, node: NodeKey) -> DirtyFlags {
    let item = tree.inputs.item_for_node(node).expect("laid out");
    tree.dirty.entry(item).flags
}

#[tokio::test]
async fn builds_nodes_with_initial_values() {
    run(async || {
        let tree = shared();
        let cx = UiCx::new(tree.clone());
        let (size, _) = signal(12.0);

        let root = cx.build(|cx| {
            cx.div()
                .style((Padding(Rect::new_all(StyleUnit::Px(4.0))),))
                .style_with(move || FontSize(StyleUnit::Px(size.get())))
                .child(|cx| cx.text("a").build())
                .child(|cx| cx.text_with(|| "b".to_owned()).build())
                .build()
        });

        let tree = tree.borrow();
        let ui = &tree.ui;
        assert_eq!(ui.kind(root), Some(Node::Box));
        assert_eq!(ui.parent(root), None);
        assert_eq!(
            ui.prop::<FontSize>(root).as_deref(),
            Some(&FontSize(StyleUnit::Px(12.0)))
        );

        let children: Vec<_> = ui.children(root).collect();
        let texts: Vec<_> = children
            .iter()
            .map(|&child| ui.prop::<Text>(child).unwrap().0.clone())
            .collect();
        assert_eq!(texts, ["a", "b"]);
        // Initial values are not changes.
        drop(tree);
        assert!(!cx.tree().borrow_mut().take_redraw());
    })
    .await;
}

#[tokio::test]
async fn bindings_write_changes_and_mark_flags() {
    run(async || {
        let tree = shared();
        let cx = UiCx::new(tree.clone());
        let (size, set_size) = signal(12.0);
        let (pad, set_pad) = signal(0.0);
        let (red, set_red) = signal(0u8);

        let mut text = None;
        let root = cx.build(|cx| {
            cx.div()
                .style_with(move || FontSize(StyleUnit::Px(size.get())))
                .style_with(move || Padding(Rect::new_all(StyleUnit::Px(pad.get()))))
                .child(|cx| {
                    text = Some(
                        cx.text("text")
                            .style_with(move || TextColor(paint(red.get())))
                            .build(),
                    );
                })
                .build()
        });
        let text = text.unwrap();

        let mut fragments = FragmentTree::new();
        layout(&mut tree.borrow_mut(), root, &mut fragments);

        // Repaint only: nothing is marked, but a redraw is requested.
        set_red.set(255);
        settle().await;
        {
            let mut tree = tree.borrow_mut();
            assert_eq!(
                *tree.ui.prop::<TextColor>(text).unwrap(),
                TextColor(paint(255))
            );
            assert_eq!(flags(&tree, text), DirtyFlags::empty());
            assert!(tree.take_redraw());
        }

        set_pad.set(8.0);
        settle().await;
        assert_eq!(flags(&tree.borrow(), root), DirtyFlags::NEED_BOX_LAYOUT);
        layout(&mut tree.borrow_mut(), root, &mut fragments);

        set_size.set(20.0);
        settle().await;
        assert_eq!(flags(&tree.borrow(), root), DirtyFlags::NEED_FULL_LAYOUT);
        assert!(tree.borrow_mut().take_redraw());

        // Equal values are skipped.
        set_size.set(20.0);
        settle().await;
        assert!(!tree.borrow_mut().take_redraw());
    })
    .await;
}

#[tokio::test]
async fn text_changes_relayout() {
    run(async || {
        let tree = shared();
        let cx = UiCx::new(tree.clone());
        let (label, set_label) = signal("short".to_owned());

        let mut badge = None;
        let root = cx.build(|cx| {
            cx.div()
                .child(|cx| {
                    badge = Some(
                        cx.div()
                            .style((DisplayOuter::Inline, DisplayInner::FlowRoot))
                            .child(move |cx| cx.text_with(move || label.get()).build())
                            .build(),
                    );
                })
                .build()
        });
        let badge = badge.unwrap();

        let mut fragments = FragmentTree::new();
        let width = |tree: &UiTree, fragments: &FragmentTree| {
            let item = tree.inputs.item_for_node(badge).unwrap();
            fragments.items[fragments.box_fragment(item).unwrap()]
                .layout
                .size
                .width
        };

        layout(&mut tree.borrow_mut(), root, &mut fragments);
        let short = width(&tree.borrow(), &fragments);

        set_label.set("a much longer label".to_owned());
        settle().await;
        layout(&mut tree.borrow_mut(), root, &mut fragments);
        let long = width(&tree.borrow(), &fragments);

        assert!(long > short, "{long} > {short}");
    })
    .await;
}

#[tokio::test]
async fn helpers_bind_values_and_signals() {
    run(async || {
        let tree = shared();
        let cx = UiCx::new(tree.clone());
        let (size, set_size) = signal(StyleUnit::Px(12.0));
        let gap = Size::new(StyleUnit::Px(4.0), StyleUnit::Px(4.0));

        let root = cx.build(|cx| {
            cx.div()
                .flex()
                .gap(gap)
                .font_size(size)
                .padding(axis(px(2), percent(50)))
                .min_size(comet_reactivity::prelude::size(auto(), vh(100)))
                .build()
        });

        let font_size = |tree: &UiTree| *tree.ui.prop::<FontSize>(root).unwrap();
        {
            let tree = tree.borrow();
            assert_eq!(*tree.ui.prop::<Gap>(root).unwrap(), Gap(gap));
            assert!(tree.ui.prop::<DisplayInner>(root).is_some());
            assert_eq!(font_size(&tree), FontSize(StyleUnit::Px(12.0)));
            assert_eq!(
                *tree.ui.prop::<Padding>(root).unwrap(),
                Padding(Rect::new_axis(StyleUnit::Px(2.0), StyleUnit::Percent(0.5)))
            );
            assert_eq!(
                *tree.ui.prop::<MinSize>(root).unwrap(),
                MinSize(Size::new(
                    AutoValue::Auto,
                    AutoValue::Value(StyleUnit::Vh(1.0))
                ))
            );
        }

        set_size.set(StyleUnit::Px(20.0));
        settle().await;
        assert_eq!(font_size(&tree.borrow()), FontSize(StyleUnit::Px(20.0)));
    })
    .await;
}

#[tokio::test]
async fn dropping_the_owner_stops_bindings() {
    run(async || {
        let tree = shared();
        let (size, set_size) = signal(12.0);

        let root = {
            let cx = UiCx::new(tree.clone());
            cx.build(|cx| {
                cx.div()
                    .style_with(move || FontSize(StyleUnit::Px(size.get())))
                    .build()
            })
        };

        set_size.set(20.0);
        settle().await;

        let tree = tree.borrow();
        assert_eq!(
            *tree.ui.prop::<FontSize>(root).unwrap(),
            FontSize(StyleUnit::Px(12.0))
        );
    })
    .await;
}

#[tokio::test]
async fn unbuilt_nodes_are_released() {
    run(async || {
        let tree = shared();
        let cx = UiCx::new(tree.clone());
        let (size, set_size): (ReadSignal<f64>, _) = signal(12.0);

        let key = {
            let build = cx
                .div()
                .style_with(move || FontSize(StyleUnit::Px(size.get())));
            build.key()
        };

        set_size.set(20.0);
        settle().await;
        assert_eq!(tree.borrow().ui.kind(key), None);
        assert!(!tree.borrow_mut().take_redraw());
    })
    .await;
}

fn paint(red: u8) -> Paint {
    Paint::Solid(Color::from_rgb8(red, 0, 0))
}
