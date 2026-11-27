//! Layout by the fragment tree builder.

use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};

use super::builder::{Fixture, VIEWPORT, paragraph, text};
use crate::{
    dirty_map::DirtyFlags,
    fragment::{BoxFragmentLink, BoxFragmentType, FragmentGeometry, FragmentRef, InlineLayout},
    style::bx,
};
use comet_geometry::{AutoValue, Point, Rect, Size, StyleUnit};

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

    // Each piece has an inline part box decorating it, linked in order.
    let piece_boxes: Vec<_> = pieces
        .iter()
        .map(|&piece| inline.items[piece].child_box().unwrap())
        .collect();
    assert_eq!(
        fx.fragments.split_boxes(piece_boxes[0]).collect::<Vec<_>>(),
        piece_boxes
    );
    for &key in &piece_boxes {
        let fragment = &fx.fragments.boxes[key];
        assert_eq!(fragment.box_type, BoxFragmentType::InlinePart);
        assert_eq!(fragment.source, span);
        assert!(fx.fragments.is_anonymous(key));
    }

    for &piece in &pieces {
        let item = &inline.items[piece];
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
fn blocks_in_inline_content_are_laid_out_by_the_block_algorithm() {
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

    let root_box = fx.box_of(root);
    assert!(fx.fragments.boxes[root_box].child_layout.is_mixed_inline());
    assert_eq!(fx.run_texts(root), ["before", "after"]);
    assert_eq!(
        fx.fragments.boxes[fx.box_of(first)].box_type,
        BoxFragmentType::Normal
    );

    let runs: Vec<_> = fx.fragments.runs(root_box).collect();
    let run_height = fx.fragments.boxes[runs[0]].layout.geometry.size.height;
    let (first, second) = (fx.geometry(first), fx.geometry(second));
    assert_eq!(first.size.width, VIEWPORT.width, "blocks fill the line");
    assert_eq!(first.location.x, 0.0);
    assert!(
        (first.location.y - (run_height + 10.0)).abs() < 1e-3,
        "below the first run and its margin"
    );

    let gap = second.location.y - (first.location.y + first.size.height);
    assert!(
        (gap - 10.0).abs() < 1e-3,
        "margins collapse, got a gap of {gap}"
    );

    let last_run = fx.fragments.boxes[runs[1]].layout.geometry;
    assert!((last_run.location.y - (second.location.y + second.size.height + 10.0)).abs() < 1e-3);
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

/// A paragraph `p` holding `<span>before<div>block</div>after</span> tail`.
struct Split {
    root: NodeKey,
    span: NodeKey,
    block: NodeKey,
}

fn split_paragraph(ui: &mut Ui) -> Split {
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let before = text(ui, "before");
    let block = paragraph(ui, "block");
    let after = text(ui, "after");
    let tail = text(ui, " tail");
    ui.append(root, span);
    ui.append(span, before);
    ui.append(span, block);
    ui.append(span, after);
    ui.append(root, tail);

    Split { root, span, block }
}

#[test]
fn runs_are_linked_and_split_elements_continue_across_them() {
    let mut ui = Ui::new();
    let Split { root, span, block } = split_paragraph(&mut ui);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    let root_box = fx.box_of(root);
    assert!(fx.fragments.boxes[root_box].child_layout.is_mixed_inline());
    let runs: Vec<_> = fx.fragments.runs(root_box).collect();
    assert_eq!(runs.len(), 2);
    assert_eq!(fx.children(root_box), [runs[0], fx.box_of(block), runs[1]]);
    assert_eq!(
        fx.fragments.boxes[runs[0]].link,
        BoxFragmentLink {
            prev: None,
            next: Some(runs[1])
        }
    );
    assert_eq!(fx.fragments.split_boxes(runs[1]).collect::<Vec<_>>(), runs);
    assert_eq!(
        fx.fragments.boxes[fx.box_of(block)].link,
        BoxFragmentLink::default()
    );

    let span = fx.item(span);
    assert!(fx.fragments.is_split_after(&fx.tree, runs[0], span));
    assert!(fx.fragments.is_split_before(&fx.tree, runs[1], span));
    assert!(!fx.fragments.is_split_before(&fx.tree, runs[0], span));
    assert!(!fx.fragments.is_split_after(&fx.tree, runs[1], span));
    let root_item = fx.item(root);
    assert!(
        !fx.fragments.is_split_before(&fx.tree, runs[1], root_item),
        "the owner is not split"
    );
    assert_eq!(fx.fragments.inline_owner(&fx.tree, span), Some(runs[0]));

    let pieces: Vec<_> = fx.fragments.fragments(&fx.tree, span).collect();
    let owners: Vec<_> = pieces
        .iter()
        .map(|piece| match *piece {
            FragmentRef::Inline { owner, .. } => owner,
            FragmentRef::Box(_) => panic!("the span has no box"),
        })
        .collect();
    assert_eq!(owners, runs, "one piece in each run, in order");
}

#[test]
fn inserting_a_block_splits_a_run_keeping_keys() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before ");
    let atomic = badge(&mut ui, 30.0, 10.0);
    let after = text(&mut ui, " after");
    ui.append(root, before);
    ui.append(root, atomic);
    ui.append(root, after);

    let mut fx = Fixture::new(ui, root);
    fx.layout();
    let (root_box, atomic_box) = (fx.box_of(root), fx.box_of(atomic));
    assert!(fx.fragments.boxes[root_box].child_layout.is_inline());

    // A block before the atomic inline moves it to a new run.
    fx.next_round();
    let block = paragraph(&mut fx.ui, "block");
    fx.ui.before(atomic, block);
    fx.mark(root, DirtyFlags::NEED_CHILDREN);
    fx.layout();

    assert_eq!(fx.box_of(root), root_box);
    assert!(fx.fragments.boxes[root_box].child_layout.is_mixed_inline());
    assert_eq!(fx.run_texts(root), ["before ", "\u{FFFC} after"]);
    assert_eq!(fx.box_of(atomic), atomic_box, "moved with its key");
    let runs: Vec<_> = fx.fragments.runs(root_box).collect();
    assert_eq!(fx.fragments.boxes.parent(atomic_box), Some(runs[1]));
    let first_run = runs[0];

    // Editing the second run leaves the first one built.
    fx.next_round();
    fx.ui
        .set_props(after, (comet_tree::node::Text(" later".into()),));
    fx.mark(after, DirtyFlags::NEED_FULL_LAYOUT);
    fx.layout();
    assert_eq!(fx.run_texts(root), ["before ", "\u{FFFC} later"]);
    assert_eq!(fx.fragments.runs(root_box).next(), Some(first_run));

    // Removing the block makes it one inline layout again.
    fx.next_round();
    fx.ui.delete(block);
    fx.mark(root, DirtyFlags::NEED_CHILDREN);
    fx.layout();

    assert_eq!(fx.box_of(root), root_box);
    assert!(fx.fragments.boxes[root_box].child_layout.is_inline());
    assert_eq!(fx.inline(root).text, "before \u{FFFC} later");
    assert_eq!(fx.box_of(atomic), atomic_box);
    assert_eq!(fx.fragments.boxes.parent(atomic_box), Some(root_box));
}

#[test]
fn block_margins_collapse_through_a_mixed_owner() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let owner = ui.create(Node::Box, ());
    let block = paragraph(&mut ui, "first");
    let tail = text(&mut ui, "tail");
    vertical_margin(&mut ui, block, 10.0);
    ui.append(root, owner);
    ui.append(owner, block);
    ui.append(owner, tail);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    assert!(
        fx.fragments.boxes[fx.box_of(owner)]
            .child_layout
            .is_mixed_inline()
    );
    assert_eq!(
        fx.geometry(owner).location.y,
        10.0,
        "the margin collapses through"
    );
    assert_eq!(fx.geometry(block).location.y, 0.0);
}

#[test]
fn blocks_in_inline_content_can_be_flow_roots() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before");
    let block = paragraph(&mut ui, "block");
    ui.set_props(block, (bx::DisplayInner::FlowRoot,));
    ui.append(root, before);
    ui.append(root, block);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    assert_eq!(
        fx.fragments.boxes[fx.box_of(block)].box_type,
        BoxFragmentType::FlowRoot
    );
}

#[test]
fn absolute_boxes_in_runs_bubble_through_the_owner() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let Split {
        root: owner,
        span,
        block,
    } = split_paragraph(&mut ui);
    unpositioned(&mut ui, owner);
    unpositioned(&mut ui, span);
    let abs = absolute(&mut ui, Some((5.0, 7.0)));
    let first = paragraph(&mut ui, "first");
    ui.append(root, first);
    ui.append(root, owner);
    ui.append(span, abs);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    assert!(fx.geometry(owner).location.y > 0.0);
    assert_eq!(fx.location_in(abs, root), (5.0, 7.0));
    assert_eq!(
        fx.fragments.boxes[fx.box_of(root)].hoisted,
        [fx.box_of(abs)]
    );
    let _ = block;
}

#[test]
fn runs_with_only_out_of_flow_boxes_take_no_space() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    relative(&mut ui, root);
    let before = text(&mut ui, "before");
    let block = paragraph(&mut ui, "block");
    let abs = absolute(&mut ui, Some((5.0, 7.0)));
    ui.append(root, before);
    ui.append(root, block);
    ui.append(root, abs);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    let runs: Vec<_> = fx.fragments.runs(fx.box_of(root)).collect();
    assert_eq!(fx.run_texts(root), ["before", ""]);
    assert_eq!(fx.fragments.boxes[runs[1]].layout.geometry.size.height, 0.0);
    assert_eq!(
        fx.geometry(root).size.height,
        fx.geometry(block).location.y + fx.geometry(block).size.height
    );
    assert_eq!(fx.location_in(abs, root), (5.0, 7.0));
}

/// A paragraph of `width` pixels holding `before <span>{inner}</span>`.
fn paragraph_with_span(ui: &mut Ui, width: f64, inner: &str) -> (NodeKey, NodeKey) {
    let root = fixed(ui, width, 400.0);
    let before = text(ui, "before ");
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let inner = text(ui, inner);
    ui.append(root, before);
    ui.append(root, span);
    ui.append(span, inner);

    (root, span)
}

#[test]
fn piece_boxes_follow_the_lines() {
    let mut ui = Ui::new();
    let (root, span) = paragraph_with_span(&mut ui, 120.0, "one two three four");

    let mut fx = Fixture::new(ui, root);
    fx.layout();
    let span_item = fx.item(span);
    let pieces: Vec<_> = fx.fragments.inline_boxes(&fx.tree, span_item).collect();
    assert!(pieces.len() > 1);
    assert!(
        pieces
            .iter()
            .all(|&key| fx.fragments.boxes.parent(key) == Some(fx.box_of(root)))
    );

    // Narrower: more pieces, created again and linked in order; the old
    // boxes are gone.
    fx.next_round();
    fx.ui.set_props(
        root,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Px(60.0)),
            AutoValue::Value(StyleUnit::Px(400.0)),
        )),),
    );
    fx.mark(root, DirtyFlags::NEED_BOX_LAYOUT);
    fx.layout();

    let narrow: Vec<_> = fx.fragments.inline_boxes(&fx.tree, span_item).collect();
    assert!(narrow.len() > pieces.len());
    assert!(pieces.iter().all(|&key| fx.fragments.get(key).is_none()));
    assert_eq!(fx.fragments.boxes[narrow[0]].link.prev, None);
    assert_eq!(fx.fragments.boxes[*narrow.last().unwrap()].link.next, None);
}

#[test]
fn piece_boxes_are_linked_across_runs() {
    let mut ui = Ui::new();
    let Split { root, span, block } = split_paragraph(&mut ui);

    let mut fx = Fixture::new(ui, root);
    fx.layout();
    let span_item = fx.item(span);
    let runs: Vec<_> = fx.fragments.runs(fx.box_of(root)).collect();
    let pieces: Vec<_> = fx.fragments.inline_boxes(&fx.tree, span_item).collect();
    assert_eq!(pieces.len(), 2);
    assert_eq!(fx.fragments.boxes.parent(pieces[0]), Some(runs[0]));
    assert_eq!(fx.fragments.boxes.parent(pieces[1]), Some(runs[1]));

    // Emitting only the second run again links its new piece to the first.
    fx.next_round();
    let after = fx.ui.next_sibling(block).unwrap();
    fx.ui
        .set_props(after, (comet_tree::node::Text("changed".into()),));
    fx.mark(after, DirtyFlags::NEED_FULL_LAYOUT);
    fx.layout();

    let relinked: Vec<_> = fx.fragments.inline_boxes(&fx.tree, span_item).collect();
    assert_eq!(relinked.len(), 2);
    assert_eq!(fx.fragments.boxes.parent(relinked[1]), Some(runs[1]));
    assert_eq!(fx.fragments.boxes[relinked[0]].link.next, Some(relinked[1]));
    assert_eq!(fx.fragments.boxes[relinked[1]].link.prev, Some(relinked[0]));
}

#[test]
fn piece_boxes_take_their_element_style() {
    let mut ui = Ui::new();
    let (root, span) = paragraph_with_span(&mut ui, 400.0, "inner");

    let mut fx = Fixture::new(ui, root);
    fx.layout();
    let span_item = fx.item(span);
    let piece = |fx: &Fixture| {
        fx.fragments
            .inline_boxes(&fx.tree, span_item)
            .next()
            .unwrap()
    };
    let style = fx.fragments.boxes[piece(&fx)].style().clone();

    fx.next_round();
    let px = StyleUnit::Px(4.0);
    fx.ui.set_props(
        span,
        (bx::Padding(Rect::new(
            Point::new(px, px),
            Point::new(px, px),
        )),),
    );
    fx.mark(span, DirtyFlags::NEED_BOX_LAYOUT);
    fx.layout();

    assert_ne!(fx.fragments.boxes[piece(&fx)].style(), &style);
}

#[test]
fn empty_elements_have_a_piece_box() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before");
    let empty = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    ui.append(root, before);
    ui.append(root, empty);

    let mut fx = Fixture::new(ui, root);
    fx.layout();

    let empty_item = fx.item(empty);
    let pieces: Vec<_> = fx.fragments.inline_boxes(&fx.tree, empty_item).collect();
    assert_eq!(pieces.len(), 1);
    let geometry = fx.fragments.boxes[pieces[0]].layout.geometry;
    assert_eq!(geometry.size.width, 0.0);
    assert!(geometry.location.x > 0.0, "after the text");
}
