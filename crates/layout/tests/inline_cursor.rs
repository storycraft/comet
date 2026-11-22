use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{FragmentItem, FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    inline::{InlineCursor, InlinePosition},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Size, StyleUnit},
    },
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

const VIEWPORT: Size<f64> = Size::new(400.0, 600.0);
const NARROW: Size<f64> = Size::new(80.0, 600.0);

struct Fixture {
    inputs: LayoutInputTreeBuilder,
    tree: LayoutInputTree,
    dirty: DirtyMap,
    builder: FragmentTreeBuilder,
    fragments: FragmentTree,
}

impl Fixture {
    fn new() -> Self {
        Self {
            inputs: LayoutInputTreeBuilder::new(),
            tree: LayoutInputTree::new(),
            dirty: DirtyMap::new(),
            builder: FragmentTreeBuilder::new(),
            fragments: FragmentTree::new(),
        }
    }

    fn build(
        &mut self,
        ui: &Ui,
        node: NodeKey,
        viewport: Size<f64>,
    ) -> (LayoutInputKey, FragmentKey) {
        let root = self
            .inputs
            .build(
                ui,
                &mut self.tree,
                &mut self.dirty,
                BuildRequest::new(node, 16.0, viewport),
            )
            .unwrap();
        let fragment = self.layout(ui, root, viewport);

        (root, fragment)
    }

    fn layout(&mut self, ui: &Ui, root: LayoutInputKey, viewport: Size<f64>) -> FragmentKey {
        let fragment = self
            .builder
            .build(
                ui,
                &self.tree,
                &mut self.dirty,
                root,
                &mut self.fragments,
                &LayoutConstraint::new_root(viewport, 16.0),
            )
            .unwrap();
        self.dirty.clear(&self.tree, root);

        fragment
    }

    fn key(&self, node: NodeKey) -> LayoutInputKey {
        self.tree.item_for_node(node).unwrap()
    }

    fn children(&self, key: FragmentKey) -> Vec<FragmentKey> {
        self.fragments.items.children(key).collect()
    }

    fn text(&self, cursor: &InlineCursor) -> &str {
        &cursor.inline(&self.fragments).unwrap().text[cursor.text_range()]
    }

    /// Every run of an owner, walking forward from its start.
    fn walk(&self, owner: FragmentKey) -> Vec<InlineCursor> {
        std::iter::successors(InlineCursor::start(&self.fragments, owner), |cursor| {
            cursor.next_run(&self.fragments)
        })
        .collect()
    }
}

fn text(ui: &mut Ui, value: &str) -> NodeKey {
    ui.create(Node::Text, (Text(value.into()),))
}

fn span(ui: &mut Ui) -> NodeKey {
    ui.create(Node::Box, (bx::DisplayOuter::Inline,))
}

fn atomic(ui: &mut Ui, width: f64) -> NodeKey {
    let px = |value| AutoValue::Value(StyleUnit::Px(value));
    ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            bx::DisplayInner::FlowRoot,
            bx::Size(Size::new(px(width), px(10.0))),
        ),
    )
}

fn sized_block(ui: &mut Ui, height: f64) -> NodeKey {
    ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Auto,
            AutoValue::Value(StyleUnit::Px(height)),
        )),),
    )
}

/// A box holding one text.
fn paragraph(ui: &mut Ui, value: &str) -> (NodeKey, NodeKey) {
    let root = ui.create(Node::Box, ());
    let content = text(ui, value);
    ui.append(root, content);

    (root, content)
}

const WRAPPING: &str = "several words that wrap at narrow widths";

#[test]
fn runs_cover_the_text_in_logical_order_across_lines() {
    let mut ui = Ui::new();
    let (root, _) = paragraph(&mut ui, WRAPPING);
    let mut f = Fixture::new();
    let (_, owner) = f.build(&ui, root, NARROW);

    let lines = f.children(owner);
    assert!(lines.len() > 1);

    let runs = f.walk(owner);
    assert!(runs.len() >= lines.len());
    assert_eq!(runs[0].index(), 0);
    assert_eq!(runs.last().unwrap().text_range().end, WRAPPING.len());

    let mut previous_end = 0;
    let mut line = 0;
    for cursor in &runs {
        assert_eq!(cursor.owner(), owner);
        assert_eq!(cursor.index(), cursor.text_range().start);
        assert_eq!(cursor.text_range().start, previous_end);
        previous_end = cursor.text_range().end;

        assert!(matches!(
            f.fragments.items[cursor.fragment()].item,
            FragmentItem::GlyphRun { .. }
        ));
        let at = lines
            .iter()
            .position(|&key| key == cursor.line_fragment())
            .unwrap();
        assert!(at >= line);
        line = at;
        assert_eq!(
            f.fragments.items.parent(cursor.fragment()),
            Some(cursor.line_fragment())
        );
    }

    // Walking back visits the same runs.
    let back: Vec<_> = std::iter::successors(runs.last().cloned(), |cursor| {
        cursor.previous_run(&f.fragments)
    })
    .collect();
    assert_eq!(back.into_iter().rev().collect::<Vec<_>>(), runs);
}

#[test]
fn positions_resolve_to_the_run_containing_them() {
    let mut ui = Ui::new();
    let (root, _) = paragraph(&mut ui, WRAPPING);
    let mut f = Fixture::new();
    let (_, owner) = f.build(&ui, root, NARROW);
    let lines = f.children(owner);

    for index in 0..WRAPPING.len() {
        let cursor = InlineCursor::new(&f.fragments, InlinePosition::new(owner, index)).unwrap();
        assert_eq!(cursor.index(), index);
        assert!(cursor.text_range().contains(&index), "{index}");
    }

    // The index at a soft break belongs to the run starting the next line.
    let second = f
        .walk(owner)
        .into_iter()
        .find(|cursor| cursor.line_fragment() == lines[1])
        .unwrap();
    let at_break = second.with_index(&f.fragments, second.index()).unwrap();
    assert_eq!(at_break.line_fragment(), lines[1]);

    let end = InlineCursor::end(&f.fragments, owner).unwrap();
    assert_eq!(end.index(), WRAPPING.len());
    assert_eq!(end.fragment(), f.walk(owner).last().unwrap().fragment());
    assert_eq!(
        InlineCursor::new(&f.fragments, InlinePosition::new(owner, WRAPPING.len())),
        Some(end)
    );
    assert!(
        InlineCursor::new(&f.fragments, InlinePosition::new(owner, WRAPPING.len() + 1)).is_none()
    );
}

#[test]
fn lines_are_navigated_by_horizontal_position() {
    let mut ui = Ui::new();
    let (root, _) = paragraph(&mut ui, WRAPPING);
    let mut f = Fixture::new();
    let (_, owner) = f.build(&ui, root, NARROW);
    let lines = f.children(owner);

    let start = InlineCursor::start(&f.fragments, owner).unwrap();
    assert!(start.previous_line(&f.fragments).is_none());

    let mut cursor = start.clone();
    for &line in &lines[1..] {
        cursor = cursor.next_line(&f.fragments).unwrap();
        assert_eq!(cursor.line_fragment(), line);
    }
    assert!(cursor.next_line(&f.fragments).is_none());

    for &line in lines[..lines.len() - 1].iter().rev() {
        cursor = cursor.previous_line_at(&f.fragments, 0.0).unwrap();
        assert_eq!(cursor.line_fragment(), line);
    }
    assert_eq!(cursor, start);
}

#[test]
fn line_edges_are_its_logically_first_and_last_runs() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let first = text(&mut ui, "first ");
    let middle = span(&mut ui);
    let inner = text(&mut ui, "middle");
    let last = text(&mut ui, " last");
    ui.append(root, first);
    ui.append(root, middle);
    ui.append(middle, inner);
    ui.append(root, last);

    let mut f = Fixture::new();
    let (_, owner) = f.build(&ui, root, VIEWPORT);
    let runs = f.walk(owner);
    let texts: Vec<_> = runs.iter().map(|cursor| f.text(cursor)).collect();
    assert_eq!(texts, ["first ", "middle", " last"]);

    let middle_run = &runs[1];
    assert_eq!(middle_run.line_start(&f.fragments), Some(runs[0].clone()));
    assert_eq!(middle_run.line_end(&f.fragments), Some(runs[2].clone()));
    assert_eq!(middle_run.source(&f.fragments), Some(f.key(inner)));

    // Glyph runs inside a span hang below its span fragment.
    let parent = f.fragments.items.parent(middle_run.fragment()).unwrap();
    assert_eq!(f.fragments.items[parent].source, f.key(middle));
}

#[test]
fn inline_boxes_are_runs_of_their_replacement_character() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before ");
    let badge = atomic(&mut ui, 30.0);
    let empty = span(&mut ui);
    let after = text(&mut ui, " after");
    ui.append(root, before);
    ui.append(root, badge);
    ui.append(root, empty);
    ui.append(root, after);

    let mut f = Fixture::new();
    let (_, owner) = f.build(&ui, root, VIEWPORT);
    let runs = f.walk(owner);
    let kinds: Vec<_> = runs
        .iter()
        .map(|cursor| match f.fragments.items[cursor.fragment()].item {
            FragmentItem::GlyphRun { .. } => "glyphs",
            FragmentItem::Box { .. } => "atomic",
            FragmentItem::Inline { .. } => "empty",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["glyphs", "atomic", "empty", "glyphs"]);

    for cursor in &runs[1..3] {
        assert_eq!(f.text(cursor), "\u{FFFC}");
    }

    let badge_box = f.fragments.box_fragment(f.key(badge)).unwrap();
    assert_eq!(runs[1].fragment(), badge_box);
    assert_eq!(runs[1].source(&f.fragments), Some(f.key(badge)));
    assert_eq!(
        InlineCursor::at_item(&f.fragments, f.key(badge)),
        Some(runs[1].clone())
    );
    assert_eq!(
        InlineCursor::from_fragment(&f.fragments, badge_box),
        Some(runs[1].clone())
    );
    assert_eq!(
        InlineCursor::at_item(&f.fragments, f.key(empty)),
        Some(runs[2].clone())
    );

    // The atomic inline owns its own layout, out of the paragraph's runs.
    assert!(InlineCursor::start(&f.fragments, badge_box).is_none());
}

#[test]
fn items_start_at_their_first_run() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = span(&mut ui);
    let nested = span(&mut ui);
    let deep = text(&mut ui, "deep ");
    let shallow = text(&mut ui, "shallow");
    ui.append(root, outer);
    ui.append(outer, nested);
    ui.append(nested, deep);
    ui.append(outer, shallow);

    let mut f = Fixture::new();
    let (input_root, owner) = f.build(&ui, root, VIEWPORT);
    let runs = f.walk(owner);

    assert_eq!(
        InlineCursor::at_item(&f.fragments, f.key(outer)),
        Some(runs[0].clone())
    );
    assert_eq!(
        InlineCursor::at_item(&f.fragments, f.key(nested)),
        Some(runs[0].clone())
    );
    assert_eq!(
        InlineCursor::at_item(&f.fragments, f.key(shallow)),
        Some(runs[1].clone())
    );
    assert_eq!(
        InlineCursor::at_item(&f.fragments, input_root),
        Some(runs[0].clone())
    );
}

#[test]
fn sibling_items_skip_to_their_first_run() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = text(&mut ui, "a ");
    let group = span(&mut ui);
    let b = text(&mut ui, "b ");
    let c = text(&mut ui, "c ");
    let blank = text(&mut ui, "");
    let d = text(&mut ui, "d");
    ui.append(root, a);
    ui.append(root, group);
    ui.append(group, b);
    ui.append(group, c);
    ui.append(root, blank);
    ui.append(root, d);

    let mut f = Fixture::new();
    let (_, owner) = f.build(&ui, root, VIEWPORT);
    let tree = &f.fragments;
    let at = |node| InlineCursor::at_item(tree, f.key(node)).unwrap();

    assert_eq!(at(a).next_sibling_item(tree, &f.tree), Some(at(b)));
    assert_eq!(at(b).next_sibling_item(tree, &f.tree), Some(at(c)));
    assert_eq!(at(c).next_sibling_item(tree, &f.tree), None);
    assert_eq!(at(b).previous_sibling_item(tree, &f.tree), None);
    // Items without runs are skipped.
    assert_eq!(at(d).previous_sibling_item(tree, &f.tree), Some(at(b)));
    assert_eq!(at(d).next_sibling_item(tree, &f.tree), None);
    assert_eq!(at(a).previous_sibling_item(tree, &f.tree), None);
    assert_eq!(at(a).owner(), owner);
}

#[test]
fn runs_of_mixed_content_stay_in_their_anonymous_block() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before");
    let block = sized_block(&mut ui, 20.0);
    let after = text(&mut ui, "after");
    ui.append(root, before);
    ui.append(root, block);
    ui.append(root, after);

    let mut f = Fixture::new();
    let (input_root, root_fragment) = f.build(&ui, root, VIEWPORT);
    let children = f.children(root_fragment);
    assert!(f.fragments.items[children[0]].item.is_anonymous_block());

    assert!(InlineCursor::start(&f.fragments, root_fragment).is_none());

    let first = InlineCursor::at_item(&f.fragments, input_root).unwrap();
    assert_eq!(first.owner(), children[0]);
    assert_eq!(first.next_run(&f.fragments), None);
    assert_eq!(first.next_sibling_item(&f.fragments, &f.tree), None);

    let second = InlineCursor::at_item(&f.fragments, f.key(after)).unwrap();
    assert_eq!(second.owner(), children[2]);
    assert_eq!(f.text(&second), "after");
}

#[test]
fn cursors_are_created_again_from_their_position_after_a_build() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let content = text(&mut ui, "hello ");
    let badge = atomic(&mut ui, 30.0);
    ui.append(root, content);
    ui.append(root, badge);

    let mut f = Fixture::new();
    let (input_root, owner) = f.build(&ui, root, VIEWPORT);
    let at_text = InlineCursor::new(&f.fragments, InlinePosition::new(owner, 3)).unwrap();
    let at_badge = InlineCursor::at_item(&f.fragments, f.key(badge)).unwrap();
    let badge_box = at_badge.fragment();

    ui.set_props(content, (Text("hello again ".into()),));
    f.dirty
        .mark_node(&f.tree, content, DirtyFlags::NEED_FULL_LAYOUT);
    f.layout(&ui, input_root, VIEWPORT);

    // Lines were emitted again; the atomic inline keeps its box.
    assert!(f.fragments.items.get(at_text.line_fragment()).is_none());
    assert_eq!(f.fragments.box_fragment(f.key(badge)), Some(badge_box));

    let recreated = InlineCursor::new(&f.fragments, at_text.position()).unwrap();
    assert_eq!(recreated.index(), 3);
    assert_eq!(f.text(&recreated), "hello again ");

    let moved = recreated.next_run(&f.fragments).unwrap();
    assert_eq!(moved.fragment(), badge_box);
    assert_eq!(moved.index(), "hello again ".len());

    // The old badge position now lands in the longer text.
    let shifted = InlineCursor::new(&f.fragments, at_badge.position()).unwrap();
    assert_eq!(shifted.source(&f.fragments), Some(f.key(content)));

    ui.set_props(content, (Text("hi".into()),));
    ui.delete(badge);
    f.dirty.mark_node(&f.tree, root, DirtyFlags::NEED_CHILDREN);
    f.build(&ui, root, VIEWPORT);
    assert!(InlineCursor::new(&f.fragments, at_badge.position()).is_none());
}

#[test]
fn bidi_runs_follow_logical_order() {
    let value = "left \u{05E9}\u{05DC}\u{05D5}\u{05DD} \u{05E2}\u{05D5}\u{05DC}\u{05DD} right";
    let mut ui = Ui::new();
    let (root, _) = paragraph(&mut ui, value);
    let mut f = Fixture::new();
    let (_, owner) = f.build(&ui, root, VIEWPORT);

    let runs = f.walk(owner);
    assert!(runs.len() >= 3, "bidi levels split the glyph runs");

    let mut previous_end = 0;
    for cursor in &runs {
        assert_eq!(cursor.text_range().start, previous_end);
        previous_end = cursor.text_range().end;
    }
    assert_eq!(previous_end, value.len());
}

#[test]
fn points_hit_carets_in_their_glyph_run() {
    let mut ui = Ui::new();
    let (root, _) = paragraph(&mut ui, WRAPPING);
    let mut f = Fixture::new();
    let (_, owner) = f.build(&ui, root, NARROW);

    for run in f.walk(owner) {
        let size = f.fragments.items[run.fragment()].layout.size;
        let y = size.height / 2.0;

        // Left of the run, and on the left half of its first cluster.
        for x in [-10.0, 1.0] {
            let hit =
                InlineCursor::from_point(&f.fragments, run.fragment(), kurbo::Point::new(x, y));
            assert_eq!(hit, Some(run.clone()));
        }

        // Past the right edge, the caret is at the run's end, in the run.
        let hit = InlineCursor::from_point(
            &f.fragments,
            run.fragment(),
            kurbo::Point::new(size.width + 10.0, y),
        )
        .unwrap();
        assert_eq!(hit.fragment(), run.fragment());
        assert_eq!(hit.index(), run.text_range().end);

        // Inside the run, carets fall on cluster edges within its text.
        let hit = InlineCursor::from_point(
            &f.fragments,
            run.fragment(),
            kurbo::Point::new(size.width / 2.0, y),
        )
        .unwrap();
        assert!(run.text_range().contains(&hit.index()));
        assert!(hit.index() > run.index());
    }
}

#[test]
fn points_only_hit_glyph_runs() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before ");
    let badge = atomic(&mut ui, 30.0);
    ui.append(root, before);
    ui.append(root, badge);

    let mut f = Fixture::new();
    let (_, owner) = f.build(&ui, root, VIEWPORT);
    let line = f.children(owner)[0];
    let badge_box = f.fragments.box_fragment(f.key(badge)).unwrap();
    let point = kurbo::Point::new(1.0, 1.0);

    for fragment in [owner, line, badge_box] {
        assert!(InlineCursor::from_point(&f.fragments, fragment, point).is_none());
    }
}
