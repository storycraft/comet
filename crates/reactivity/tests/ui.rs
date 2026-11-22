use std::{cell::RefCell, rc::Rc};

use any_spawner::Executor;
use anyrender::Paint;
use comet_accesskit::style::Role;
use comet_layout::{
    dirty_map::DirtyMap,
    fragment::{builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint,
        bx::{DisplayInner, DisplayOuter, FontSize, MinSize, Padding},
        geometry::{AutoValue, Rect, Size, StyleUnit},
    },
};
use comet_layout_flex::styles::Gap;
use comet_paint::style::TextColor;
use comet_reactivity::{
    PropHint, UiCx, UiTree,
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

/// The layout steps of one UI, consuming the changes recorded in it.
#[derive(Default)]
struct Layout {
    inputs: LayoutInputTree,
    dirty: DirtyMap,
    fragments: FragmentTree,
}

impl Layout {
    fn layout(&mut self, tree: &mut UiTree, root: NodeKey) {
        let viewport = Size::new(800.0, 600.0);
        let Self {
            inputs,
            dirty,
            fragments,
        } = self;
        for (node, flags) in tree.take_changes() {
            dirty.mark_node(inputs, node, flags.layout());
        }

        let ui = &tree.ui;
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
}

/// The flags recorded for `node` since the changes were last taken.
fn flags(tree: &UiTree, node: NodeKey) -> PropHint {
    tree.changes().get(&node).copied().unwrap_or_default()
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
                .children(|cx| {
                    [
                        cx.text("a").build(),
                        cx.text_with(|| "b".to_owned()).build(),
                    ]
                })
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
        // Initial values are not changes: only the new children are.
        assert_eq!(flags(&tree, root), PropHint::CHILDREN);
        assert!(children.iter().all(|&child| flags(&tree, child).is_empty()));
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
                .children(|cx| {
                    *text.insert(
                        cx.text("text")
                            .style_with(move || TextColor(paint(red.get())))
                            .build(),
                    )
                })
        });
        let text = text.unwrap();

        tree.borrow_mut().take_changes().for_each(drop);

        // Repaint only: no layout flags.
        set_red.set(255);
        settle().await;
        {
            let mut tree = tree.borrow_mut();
            assert_eq!(
                *tree.ui.prop::<TextColor>(text).unwrap(),
                TextColor(paint(255))
            );
            assert_eq!(flags(&tree, text), PropHint::RENDER);
            assert!(flags(&tree, text).layout().is_empty());
            tree.take_changes().for_each(drop);
        }

        // Flags add up until taken.
        set_pad.set(8.0);
        set_red.set(0);
        settle().await;
        assert_eq!(flags(&tree.borrow(), root), PropHint::BOX_LAYOUT);
        set_size.set(20.0);
        settle().await;
        assert_eq!(
            flags(&tree.borrow(), root),
            PropHint::FULL_LAYOUT | PropHint::BOX_LAYOUT
        );
        assert_eq!(flags(&tree.borrow(), text), PropHint::RENDER);
        assert_eq!(tree.borrow_mut().take_changes().count(), 2);

        // Equal values are skipped.
        set_size.set(20.0);
        settle().await;
        assert!(tree.borrow().changes().is_empty());
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
            cx.div().children(|cx| {
                *badge.insert(
                    cx.div()
                        .style((DisplayOuter::Inline, DisplayInner::FlowRoot))
                        .children(|cx| cx.text_with(move || label.get()).build()),
                )
            })
        });
        let badge = badge.unwrap();

        let mut layout = Layout::default();
        let width = |layout: &Layout| {
            let item = layout.inputs.item_for_node(badge).unwrap();
            let fragments = &layout.fragments;
            fragments.items[fragments.box_fragment(item).unwrap()]
                .layout
                .size
                .width
        };

        layout.layout(&mut tree.borrow_mut(), root);
        let short = width(&layout);

        set_label.set("a much longer label".to_owned());
        settle().await;
        assert_eq!(flags(&tree.borrow(), badge), PropHint::empty());
        layout.layout(&mut tree.borrow_mut(), root);
        let long = width(&layout);

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
        assert!(tree.borrow().changes().is_empty());
    })
    .await;
}

#[tokio::test]
async fn built_nodes_attach_only_through_children() {
    run(async || {
        let tree = shared();
        let cx = UiCx::new(tree.clone());

        let a = cx.text("a").build();
        let b = cx.text("b").build();
        // Built nodes have no parent until returned to `children`.
        assert_eq!(tree.borrow().ui.parent(a), None);

        let root = cx.div().children(|_| vec![b, a]);

        let tree = tree.borrow();
        assert_eq!(tree.ui.children(root).collect::<Vec<_>>(), [b, a]);
        assert_eq!(tree.ui.parent(a), Some(root));
        assert_eq!(flags(&tree, root), PropHint::CHILDREN);
    })
    .await;
}

#[tokio::test]
async fn lazy_views_build_while_attached() {
    run(async || {
        let tree = shared();
        let cx = UiCx::new(tree.clone());

        let root = cx.div().children(|cx| {
            let cx = cx.clone();
            (0..3).map(move |i| cx.text(i.to_string()).build())
        });

        let tree = tree.borrow();
        let texts: Vec<_> = tree
            .ui
            .children(root)
            .map(|child| tree.ui.prop::<Text>(child).unwrap().0.clone())
            .collect();
        assert_eq!(texts, ["0", "1", "2"]);
    })
    .await;
}

#[tokio::test]
async fn children_belong_to_a_child_owner() {
    run(async || {
        let tree = shared();
        let cx = UiCx::new(tree.clone());
        let (size, set_size) = signal(12.0);

        let mut text = None;
        cx.div().children(|child| {
            let parent = child.owner().parent().expect("child owner has a parent");
            assert!(parent == *cx.owner());
            assert!(*child.owner() != *cx.owner());

            *text.insert(
                child
                    .text("text")
                    .style_with(move || FontSize(StyleUnit::Px(size.get())))
                    .build(),
            )
        });
        let text = text.unwrap();
        let font_size = || *tree.borrow().ui.prop::<FontSize>(text).unwrap();

        // The child owner outlives `children`.
        set_size.set(20.0);
        settle().await;
        assert_eq!(font_size(), FontSize(StyleUnit::Px(20.0)));

        // And is cleaned up with its parent.
        cx.owner().cleanup();
        set_size.set(30.0);
        settle().await;
        assert_eq!(font_size(), FontSize(StyleUnit::Px(20.0)));
    })
    .await;
}

#[tokio::test]
async fn empty_views_mark_nothing() {
    run(async || {
        let tree = shared();
        let cx = UiCx::new(tree.clone());

        let root = cx.build(|cx| cx.div().children(|_| None));

        let tree = tree.borrow();
        assert_eq!(tree.ui.children(root).count(), 0);
        assert!(flags(&tree, root).is_empty());
    })
    .await;
}

#[tokio::test]
async fn accessibility_changes_are_not_layout_changes() {
    run(async || {
        let tree = shared();
        let cx = UiCx::new(tree.clone());
        let (role, set_role) = signal(accesskit::Role::Button);

        let root = cx.build(|cx| cx.div().role(role).build());
        tree.borrow_mut().take_changes().for_each(drop);

        set_role.set(accesskit::Role::Link);
        settle().await;
        let tree = tree.borrow();
        assert_eq!(
            *tree.ui.prop::<Role>(root).unwrap(),
            Role(accesskit::Role::Link)
        );
        assert_eq!(flags(&tree, root), PropHint::A11Y);
        assert!(flags(&tree, root).layout().is_empty());
    })
    .await;
}

fn paint(red: u8) -> Paint {
    Paint::Solid(Color::from_rgb8(red, 0, 0))
}
