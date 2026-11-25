//! Layout by the fragment tree builder.

use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};

use super::builder::{Fixture, VIEWPORT, paragraph, text};
use crate::{
    dirty_map::DirtyFlags,
    fragment2::{BoxFragmentType, FragmentGeometry, InlineLayout},
    style::{
        bx,
        geometry::{AutoValue, Point, Rect, Size, StyleUnit},
    },
};

/// A box of `width` by `height` pixels.
fn fixed(ui: &mut Ui, width: f64, height: f64) -> NodeKey {
    ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Px(width)),
            AutoValue::Value(StyleUnit::Px(height)),
        )),),
    )
}

/// An inline-block of `width` by `height` pixels.
fn badge(ui: &mut Ui, width: f64, height: f64) -> NodeKey {
    let node = fixed(ui, width, height);
    ui.set_props(node, (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot));

    node
}

fn vertical_margin(ui: &mut Ui, node: NodeKey, margin: f64) {
    let px = AutoValue::Value(StyleUnit::Px(margin));
    let zero = AutoValue::Value(StyleUnit::Px(0.0));
    ui.set_props(
        node,
        (bx::Margin(Rect::new(
            Point::new(zero, px),
            Point::new(zero, px),
        )),),
    );
}

impl Fixture {
    fn geometry(&self, node: NodeKey) -> FragmentGeometry {
        self.fragments.boxes[self.box_of(node)].layout.geometry
    }

    fn inline(&self, node: NodeKey) -> &InlineLayout {
        self.fragments.boxes[self.box_of(node)].inline().unwrap()
    }
}

#[test]
fn stacks_block_boxes_and_emits_lines() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = paragraph(&mut ui, "first");
    let b = paragraph(&mut ui, "second");
    ui.append(root, a);
    ui.append(root, b);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    let (root_geometry, a_geometry, b_geometry) =
        (fx.geometry(root), fx.geometry(a), fx.geometry(b));
    assert_eq!(root_geometry.size.width, VIEWPORT.width);
    assert_eq!(a_geometry.size.width, VIEWPORT.width);
    assert!(a_geometry.size.height > 0.0);
    assert_eq!(b_geometry.location.y, a_geometry.size.height);
    assert!(fx.valid(fx.box_of(root)));

    let inline = fx.inline(a);
    let leaf = fx.item(fx.ui.first_child(a).unwrap());
    assert!(inline.items[0].is_line());
    assert_eq!(inline.roots().count(), 1);
    assert_eq!(inline.items_of(leaf).count(), 1);
    assert!(
        inline
            .items
            .iter()
            .all(|item| item.is_line() || item.source == leaf)
    );
    assert!(
        fx.fragments.boxes[fx.box_of(root)]
            .layout
            .baseline
            .is_some()
    );
}

#[test]
fn wraps_text_and_links_its_runs() {
    let mut ui = Ui::new();
    let root = fixed(&mut ui, 60.0, 400.0);
    let leaf = text(&mut ui, "one two three four five six seven");
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let inner = text(&mut ui, " eight nine ten eleven");
    ui.append(root, leaf);
    ui.append(root, span);
    ui.append(span, inner);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    let inline = fx.inline(root);
    let lines = inline.roots().count();
    assert!(lines > 2, "text wraps, got {lines} lines");

    let (leaf, span, inner) = (fx.item(leaf), fx.item(span), fx.item(inner));
    let runs: Vec<_> = inline.items_of(leaf).collect();
    assert!(runs.len() > 1);
    assert!(runs.windows(2).all(|pair| {
        inline.prev(pair[1]) == Some(pair[0])
            && inline.items[pair[1]].geometry.location.y > inline.items[pair[0]].geometry.location.y
    }));

    // The span is split into pieces, one per line, containing its runs.
    let pieces: Vec<_> = inline.items_of(span).collect();
    assert!(pieces.len() > 1);
    for &piece in &pieces {
        let item = &inline.items[piece];
        assert_eq!(item.child_box(), None);
        let children: Vec<_> = inline.children(piece).collect();
        assert!(!children.is_empty());

        for child in children {
            let child = &inline.items[child];
            assert_eq!(child.source, inner);
            assert!(child.geometry.location.x >= item.geometry.location.x);
            assert!(
                child.geometry.location.x + child.geometry.size.width
                    <= item.geometry.location.x + item.geometry.size.width + 1e-6
            );
        }
    }

    // Lines cover every item with their subtrees.
    let covered: usize = inline
        .roots()
        .map(|line| 1 + inline.items[line].descendants as usize)
        .sum();
    assert_eq!(covered, inline.items.len());
}

#[test]
fn places_atomic_inlines_on_lines() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = ui.create(Node::Box, ());
    let before = text(&mut ui, "hello ");
    let atomic = badge(&mut ui, 30.0, 10.0);
    ui.append(root, para);
    ui.append(para, before);
    ui.append(para, atomic);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    let atomic_box = fx.box_of(atomic);
    let geometry = fx.geometry(atomic);
    assert_eq!((geometry.size.width, geometry.size.height), (30.0, 10.0));
    assert!(geometry.location.x > 0.0, "placed after the text");

    let inline = fx.inline(para);
    let item = inline
        .items
        .iter()
        .find(|item| item.child_box() == Some(atomic_box))
        .unwrap();
    assert_eq!(item.geometry.location, geometry.location);
    assert_eq!(item.baseline, 10.0, "the bottom edge without lines inside");
}

#[test]
fn inline_parts_take_lines_of_their_own() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before");
    let first = paragraph(&mut ui, "first");
    let second = paragraph(&mut ui, "second");
    let after = text(&mut ui, "after");
    vertical_margin(&mut ui, first, 10.0);
    vertical_margin(&mut ui, second, 10.0);
    ui.append(root, before);
    ui.append(root, first);
    ui.append(root, second);
    ui.append(root, after);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    assert_eq!(
        fx.fragments.boxes[fx.box_of(first)].box_type,
        BoxFragmentType::InlinePart
    );

    let (first, second) = (fx.geometry(first), fx.geometry(second));
    assert_eq!(
        first.size.width, VIEWPORT.width,
        "inline parts fill the line"
    );
    assert!(
        first.location.y >= 10.0,
        "below the first line and its margin"
    );
    assert_eq!(first.location.x, 0.0);

    let gap = second.location.y - (first.location.y + first.size.height);
    assert!(
        (gap - 10.0).abs() < 1e-3,
        "margins collapse, got a gap of {gap}"
    );
    assert!(fx.inline(root).roots().count() >= 4);
}

#[test]
fn box_changes_lay_out_lines_without_shaping() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = ui.create(Node::Box, ());
    let before = text(&mut ui, "hello ");
    let atomic = badge(&mut ui, 30.0, 10.0);
    let after = text(&mut ui, " world");
    ui.append(root, para);
    ui.append(para, before);
    ui.append(para, atomic);
    ui.append(para, after);

    let mut fx = Fixture::new(ui, root);
    fx.layout();
    let para_box = fx.box_of(para);
    let text_ptr = fx.inline(para).text.as_ptr();
    let after_item = fx.item(after);
    let after_x = |fx: &Fixture| {
        let inline = fx.inline(para);
        inline.items[inline.items_of(after_item).next().unwrap()]
            .geometry
            .location
            .x
    };
    let old_x = after_x(&fx);

    fx.next_round();
    fx.ui.set_props(
        atomic,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Px(80.0)),
            AutoValue::Value(StyleUnit::Px(10.0)),
        )),),
    );
    fx.mark(atomic, DirtyFlags::NEED_BOX_LAYOUT);
    fx.layout();

    assert_eq!(fx.box_of(para), para_box);
    assert_eq!(fx.inline(para).text.as_ptr(), text_ptr, "not shaped again");
    assert_eq!(fx.geometry(atomic).size.width, 80.0);
    assert!(
        (after_x(&fx) - old_x - 50.0).abs() < 1e-3,
        "the text after it moved"
    );
}

#[test]
fn unchanged_layout_is_reused() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = paragraph(&mut ui, "text");
    ui.append(root, para);

    let mut fx = Fixture::new(ui, root);
    fx.layout();
    let geometry = fx.geometry(para);

    fx.dirty.clear(&fx.tree, fx.layout_root.unwrap());
    fx.layout();

    assert_eq!(fx.geometry(para), geometry);
    assert!(
        !fx.dirty
            .entry(fx.item(para))
            .flags
            .contains(DirtyFlags::NEED_PRE_PAINT),
        "nothing moved, so nothing is painted again"
    );
}

/// An absolutely positioned box of 20 by 20 pixels, with its top-left inset
/// at `(left, top)` if given.
fn absolute(ui: &mut Ui, inset: Option<(f64, f64)>) -> NodeKey {
    let node = fixed(ui, 20.0, 20.0);
    ui.set_props(node, (bx::Position(taffy::Position::Absolute),));

    if let Some((left, top)) = inset {
        let px = |value| AutoValue::Value(StyleUnit::Px(value));
        ui.set_props(
            node,
            (bx::Inset(Rect::new(
                Point::new(px(left), px(top)),
                Point::new(AutoValue::Auto, AutoValue::Auto),
            )),),
        );
    }

    node
}

fn relative(ui: &mut Ui, node: NodeKey) {
    ui.set_props(node, (bx::Position(taffy::Position::Relative),));
}

/// Boxes are relatively positioned by default, and so the containing blocks
/// of their absolutely positioned descendants. Not this one.
fn unpositioned(ui: &mut Ui, node: NodeKey) {
    ui.set_props(node, (bx::Position(taffy::Position::Static),));
}

impl Fixture {
    /// The location of `node` relative to the border box of `ancestor`.
    fn location_in(&self, node: NodeKey, ancestor: NodeKey) -> (f64, f64) {
        let ancestor = self.box_of(ancestor);
        let mut key = self.box_of(node);
        let (mut x, mut y) = (0.0, 0.0);

        while key != ancestor {
            let location = self.fragments.boxes[key].layout.geometry.location;
            x += location.x;
            y += location.y;
            key = self.fragments.boxes.parent(key).unwrap();
        }

        (x, y)
    }
}

#[test]
fn positioned_owners_place_their_absolute_boxes() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = ui.create(Node::Box, ());
    relative(&mut ui, para);
    let before = text(&mut ui, "hello ");
    let abs = absolute(&mut ui, Some((5.0, 7.0)));
    ui.append(root, para);
    ui.append(para, before);
    ui.append(para, abs);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    let geometry = fx.geometry(abs);
    assert_eq!((geometry.size.width, geometry.size.height), (20.0, 20.0));
    assert_eq!(fx.location_in(abs, para), (5.0, 7.0));
    assert_eq!(
        fx.fragments.boxes[fx.box_of(para)].hoisted,
        [fx.box_of(abs)]
    );
    assert!(
        fx.inline(para)
            .items
            .iter()
            .all(|item| item.source != fx.item(abs)),
        "out-of-flow boxes have no inline items"
    );
}

#[test]
fn absolute_boxes_bubble_to_their_containing_block() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let container = ui.create(Node::Box, ());
    relative(&mut ui, container);
    let first = paragraph(&mut ui, "first");
    let para = ui.create(Node::Box, ());
    unpositioned(&mut ui, para);
    let before = text(&mut ui, "hello ");
    let abs = absolute(&mut ui, Some((5.0, 7.0)));
    ui.append(root, container);
    ui.append(container, first);
    ui.append(container, para);
    ui.append(para, before);
    ui.append(para, abs);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    assert!(fx.geometry(para).location.y > 0.0);
    assert_eq!(fx.location_in(abs, container), (5.0, 7.0));
    assert_eq!(
        fx.fragments.boxes[fx.box_of(container)].hoisted,
        [fx.box_of(abs)]
    );
    assert!(fx.fragments.boxes[fx.box_of(para)].hoisted.is_empty());
}

#[test]
fn unclaimed_absolute_boxes_are_placed_by_the_root() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    unpositioned(&mut ui, root);
    let first = paragraph(&mut ui, "first");
    let para = ui.create(Node::Box, ());
    unpositioned(&mut ui, para);
    let before = text(&mut ui, "hello ");
    let abs = absolute(&mut ui, Some((5.0, 7.0)));
    ui.append(root, first);
    ui.append(root, para);
    ui.append(para, before);
    ui.append(para, abs);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    assert_eq!(fx.location_in(abs, root), (5.0, 7.0));
}

#[test]
fn absolute_boxes_inside_atomic_inlines_bubble_out() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    relative(&mut ui, root);
    let para = ui.create(Node::Box, ());
    unpositioned(&mut ui, para);
    let before = text(&mut ui, "hello ");
    let atomic = badge(&mut ui, 30.0, 10.0);
    unpositioned(&mut ui, atomic);
    let abs = absolute(&mut ui, Some((5.0, 7.0)));
    ui.append(root, para);
    ui.append(para, before);
    ui.append(para, atomic);
    ui.append(atomic, abs);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    assert!(fx.geometry(atomic).location.x > 0.0);
    assert_eq!(fx.location_in(abs, root), (5.0, 7.0));
    assert_eq!(
        fx.fragments.boxes[fx.box_of(root)].hoisted,
        [fx.box_of(abs)]
    );
}

#[test]
fn absolute_boxes_without_insets_stay_at_their_static_position() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    relative(&mut ui, root);
    let before = text(&mut ui, "hello ");
    let abs = absolute(&mut ui, None);
    ui.append(root, before);
    ui.append(root, abs);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    let inline = fx.inline(root);
    let run = &inline.items[inline.items_of(fx.item(before)).next().unwrap()];
    let (x, y) = fx.location_in(abs, root);
    assert!(
        (x - (run.geometry.location.x + run.geometry.size.width)).abs() < 1e-3,
        "after the text, got {x}"
    );
    assert_eq!(y, run.geometry.location.y);
}
