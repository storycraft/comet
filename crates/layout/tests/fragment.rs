use comet_layout::{
    fragment::{FragmentItem, FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        LayoutFlags, LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Point, Rect, Size, StyleUnit},
        taffy::Position,
        parley::Alignment as TextAlign,
    },
};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

const VIEWPORT: Size<f64> = Size::new(400.0, 600.0);

struct Fixture {
    inputs: LayoutInputTreeBuilder,
    tree: LayoutInputTree,
    builder: FragmentTreeBuilder,
    fragments: FragmentTree,
}

impl Fixture {
    fn new() -> Self {
        Self {
            inputs: LayoutInputTreeBuilder::new(),
            tree: LayoutInputTree::new(),
            builder: FragmentTreeBuilder::new(),
            fragments: FragmentTree::new(),
        }
    }

    fn build(&mut self, ui: &Ui, node: NodeKey) -> (LayoutInputKey, FragmentKey) {
        let root = self
            .inputs
            .build(ui, &mut self.tree, BuildRequest::new(node, 16.0, VIEWPORT))
            .unwrap();
        let fragment = self
            .builder
            .build(
                ui,
                &self.tree,
                root,
                &mut self.fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0),
            )
            .unwrap();

        (root, fragment)
    }

    fn children(&self, key: FragmentKey) -> Vec<FragmentKey> {
        self.fragments.items.children(key).collect()
    }

    fn items(&self, key: FragmentKey) -> Vec<FragmentItem> {
        self.fragments
            .items
            .children(key)
            .map(|key| self.fragments.items[key].item.clone())
            .collect()
    }
}

fn text(ui: &mut Ui, value: &str) -> NodeKey {
    ui.create(Node::Text, (Text(value.into()),))
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

fn margin(top: f64, bottom: f64) -> bx::Margin {
    bx::Margin(Rect::new_invidiual(
        AutoValue::Value(StyleUnit::Px(top)),
        AutoValue::Value(StyleUnit::ZERO),
        AutoValue::Value(StyleUnit::Px(bottom)),
        AutoValue::Value(StyleUnit::ZERO),
    ))
}

#[test]
fn block_children_split_inline_content_into_anonymous_blocks() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before");
    let block = sized_block(&mut ui, 20.0);
    let after = text(&mut ui, "after");
    ui.append(root, before);
    ui.append(root, block);
    ui.append(root, after);

    let mut f = Fixture::new();
    let (_, root_fragment) = f.build(&ui, root);

    assert_eq!(
        f.items(root_fragment),
        vec![
            FragmentItem::AnonymousBlock,
            FragmentItem::Box,
            FragmentItem::AnonymousBlock,
        ]
    );

    let children = f.children(root_fragment);
    let first = f.fragments.items[children[0]].layout;
    let block_box = f.fragments.items[children[1]].layout;
    let second = f.fragments.items[children[2]].layout;

    // Each anonymous block holds its own lines, positioned relative to it.
    for &anonymous in &[children[0], children[2]] {
        let lines = f.children(anonymous);

        assert_eq!(f.items(anonymous), vec![FragmentItem::Line { line: 0 }]);
        assert_eq!(f.fragments.items[lines[0]].layout.location.y, 0.0);
        assert!(f.fragments.inline_layout(lines[0]).is_some());
    }

    assert!(first.size.height > 0.0);
    assert_eq!(first.size.width, VIEWPORT.width);
    assert_eq!(block_box.location.y, first.location.y + first.size.height);
    assert_eq!(block_box.size, Size::new(VIEWPORT.width, 20.0));
    assert_eq!(second.location.y, block_box.location.y + 20.0);

    let root_box = f.fragments.items[root_fragment].layout;
    assert_eq!(root_box.size.height, second.location.y + second.size.height);
    assert_eq!(
        f.fragments
            .box_fragment(f.tree.item_for_node(block).unwrap()),
        Some(children[1])
    );
}

#[test]
fn leading_and_trailing_blocks_add_no_anonymous_blocks() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let first = sized_block(&mut ui, 10.0);
    let middle = text(&mut ui, "middle");
    let last = sized_block(&mut ui, 30.0);
    ui.append(root, first);
    ui.append(root, middle);
    ui.append(root, last);

    let mut f = Fixture::new();
    let (_, root_fragment) = f.build(&ui, root);

    assert_eq!(
        f.items(root_fragment),
        vec![
            FragmentItem::Box,
            FragmentItem::AnonymousBlock,
            FragmentItem::Box,
        ]
    );

    let children = f.children(root_fragment);
    let anonymous = f.fragments.items[children[1]].layout;
    let last = f.fragments.items[children[2]].layout;

    assert_eq!(f.fragments.items[children[0]].layout.location.y, 0.0);
    assert_eq!(anonymous.location.y, 10.0);
    assert_eq!(
        last.location.y,
        anonymous.location.y + anonymous.size.height
    );
    assert_eq!(
        f.fragments.items[root_fragment].layout.size.height,
        last.location.y + 30.0
    );
}

#[test]
fn inline_elements_split_around_blocks_are_linked() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let before = text(&mut ui, "before");
    let block = sized_block(&mut ui, 20.0);
    let after = text(&mut ui, "after");
    ui.append(root, span);
    ui.append(span, before);
    ui.append(span, block);
    ui.append(span, after);

    let mut f = Fixture::new();
    let (_, root_fragment) = f.build(&ui, root);
    let children = f.children(root_fragment);

    assert_eq!(children.len(), 3);

    let first_span = f.children(f.children(children[0])[0])[0];
    let second_span = f.children(f.children(children[2])[0])[0];
    let span_key = f.tree.item_for_node(span).unwrap();

    assert_eq!(f.fragments.items[first_span].source, span_key);
    assert_eq!(f.fragments.items[second_span].source, span_key);
    assert_eq!(f.fragments.items[first_span].link.next, Some(second_span));
    assert_eq!(f.fragments.items[second_span].link.prev, Some(first_span));
    assert!(matches!(
        f.fragments.items[f.children(first_span)[0]].item,
        FragmentItem::GlyphRun { .. }
    ));
}

#[test]
fn margins_of_adjacent_blocks_in_inline_content_collapse() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let before = text(&mut ui, "before");
    let first = sized_block(&mut ui, 10.0);
    let second = sized_block(&mut ui, 10.0);
    let after = text(&mut ui, "after");
    ui.set_props(first, (margin(0.0, 20.0),));
    ui.set_props(second, (margin(30.0, 0.0),));
    ui.append(root, span);
    ui.append(span, before);
    ui.append(span, first);
    ui.append(span, second);
    ui.append(span, after);

    let mut f = Fixture::new();
    let (_, root_fragment) = f.build(&ui, root);

    assert_eq!(
        f.items(root_fragment),
        vec![
            FragmentItem::AnonymousBlock,
            FragmentItem::Box,
            FragmentItem::Box,
            FragmentItem::AnonymousBlock,
        ]
    );

    let children = f.children(root_fragment);
    let first = f.fragments.items[children[1]].layout;
    let second = f.fragments.items[children[2]].layout;

    // The larger margin separates the blocks.
    assert_eq!(second.location.y, first.location.y + 10.0 + 30.0);
}

#[test]
fn inline_content_between_blocks_keeps_both_margins() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let first = sized_block(&mut ui, 10.0);
    let middle = text(&mut ui, "middle");
    let second = sized_block(&mut ui, 10.0);
    ui.set_props(first, (margin(0.0, 20.0),));
    ui.set_props(second, (margin(30.0, 0.0),));
    ui.append(root, first);
    ui.append(root, middle);
    ui.append(root, second);

    let mut f = Fixture::new();
    let (_, root_fragment) = f.build(&ui, root);
    let children = f.children(root_fragment);
    let first = f.fragments.items[children[0]].layout;
    let anonymous = f.fragments.items[children[1]].layout;
    let second = f.fragments.items[children[2]].layout;

    assert_eq!(anonymous.location.y, first.location.y + 10.0 + 20.0);
    assert_eq!(
        second.location.y,
        anonymous.location.y + anonymous.size.height + 30.0
    );
}

#[test]
fn first_block_margin_collapses_through_mixed_content_without_padding() {
    for padding in [0.0, 5.0] {
        let mut ui = Ui::new();
        let root = ui.create(Node::Box, ());
        let paragraph = ui.create(
            Node::Box,
            (bx::Padding(Rect::new_all(StyleUnit::Px(padding))),),
        );
        let block = sized_block(&mut ui, 10.0);
        let after = text(&mut ui, "after");
        ui.set_props(block, (margin(25.0, 0.0),));
        ui.append(root, paragraph);
        ui.append(paragraph, block);
        ui.append(paragraph, after);

        let mut f = Fixture::new();
        f.build(&ui, root);

        let paragraph = f
            .fragments
            .box_fragment(f.tree.item_for_node(paragraph).unwrap())
            .unwrap();
        let block = f
            .fragments
            .box_fragment(f.tree.item_for_node(block).unwrap())
            .unwrap();

        if padding == 0.0 {
            assert_eq!(f.fragments.items[paragraph].layout.location.y, 25.0);
            assert_eq!(f.fragments.items[block].layout.location.y, 0.0);
        } else {
            assert_eq!(f.fragments.items[paragraph].layout.location.y, 0.0);
            assert_eq!(f.fragments.items[block].layout.location.y, padding + 25.0);
        }
    }
}

#[test]
fn editing_one_run_keeps_the_fragments_of_other_runs() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before");
    let block = sized_block(&mut ui, 20.0);
    let after = text(&mut ui, "after");
    ui.append(root, before);
    ui.append(root, block);
    ui.append(root, after);

    let mut f = Fixture::new();
    let (input_root, root_fragment) = f.build(&ui, root);
    f.tree.clear_flags(input_root);

    let children = f.children(root_fragment);
    let first_lines = f.children(children[0]);
    let second_lines = f.children(children[2]);

    ui.set_props(after, (Text("after, but longer".into()),));
    f.tree.mark_node_dirty(after);
    f.build(&ui, root);
    f.tree.clear_flags(input_root);

    // Anonymous blocks are reused, and only the edited run is emitted again.
    assert_eq!(f.children(root_fragment), children);
    assert_eq!(f.children(children[0]), first_lines);
    assert_ne!(f.children(children[2]), second_lines);

    // Removing the block merges the runs into one inline formatting context.
    ui.delete(block);
    f.tree.mark_node_dirty(root);
    f.build(&ui, root);

    assert_eq!(f.items(root_fragment), vec![FragmentItem::Line { line: 0 }]);
    assert!(f.fragments.items.get(children[0]).is_none());
    assert!(f.fragments.items.get(children[2]).is_none());
}

#[test]
fn updates_reuse_box_fragments_and_layouts() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let header = sized_block(&mut ui, 40.0);
    let body = ui.create(Node::Box, ());
    let content = text(&mut ui, "short");
    ui.append(root, header);
    ui.append(root, body);
    ui.append(body, content);

    let mut f = Fixture::new();
    let (input_root, root_fragment) = f.build(&ui, root);
    f.tree.clear_flags(input_root);

    let body_key = f.tree.item_for_node(body).unwrap();
    let header_fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(header).unwrap());
    let body_fragment = f.fragments.box_fragment(body_key).unwrap();
    let lines = f.children(body_fragment);
    let count = f.fragments.items.len();

    // Clean trees keep every fragment.
    assert_eq!(f.build(&ui, root), (input_root, root_fragment));
    assert_eq!(f.children(body_fragment), lines);
    assert_eq!(f.fragments.items.len(), count);

    let short_height = f.fragments.items[body_fragment].layout.size.height;

    ui.set_props(
        content,
        (Text("a much longer text that has to wrap over several lines because the viewport is only four hundred pixels wide".into()),),
    );
    f.tree.mark_node_dirty(content);

    assert_eq!(f.build(&ui, root), (input_root, root_fragment));
    assert_eq!(
        f.fragments
            .box_fragment(f.tree.item_for_node(header).unwrap()),
        header_fragment
    );
    assert_eq!(f.fragments.box_fragment(body_key), Some(body_fragment));
    assert!(f.children(body_fragment).len() > 1);
    assert!(f.fragments.items[body_fragment].layout.size.height > short_height);

    f.tree.clear_flags(input_root);
    assert!(
        f.tree
            .items
            .descendants(input_root)
            .all(|key| f.tree.items[key].flags == LayoutFlags::empty())
    );
}

#[test]
fn block_child_list_changes_reuse_box_fragments_in_order() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let list = ui.create(Node::Box, ());
    let other = ui.create(Node::Box, ());
    let nodes: Vec<_> = (0..4).map(|_| sized_block(&mut ui, 10.0)).collect();
    ui.append(root, list);
    ui.append(root, other);
    for &node in &nodes {
        ui.append(list, node);
    }

    let mut f = Fixture::new();
    let (input_root, _) = f.build(&ui, root);
    f.tree.clear_flags(input_root);

    let fragment = |f: &Fixture, node| {
        f.fragments
            .box_fragment(f.tree.item_for_node(node).unwrap())
            .unwrap()
    };
    let keys: Vec<_> = nodes.iter().map(|&node| fragment(&f, node)).collect();
    let list_fragment = fragment(&f, list);
    let other_fragment = fragment(&f, other);
    let tops = |f: &Fixture, parent| {
        f.children(parent)
            .into_iter()
            .map(|key| f.fragments.items[key].layout.location.y)
            .collect::<Vec<_>>()
    };

    // Insert in the middle.
    let inserted = sized_block(&mut ui, 10.0);
    ui.before(nodes[2], inserted);
    f.tree.mark_node_dirty(list);
    f.build(&ui, root);
    f.tree.clear_flags(input_root);

    let inserted_key = fragment(&f, inserted);
    assert_eq!(
        f.children(list_fragment),
        vec![keys[0], keys[1], inserted_key, keys[2], keys[3]]
    );
    assert_eq!(tops(&f, list_fragment), vec![0.0, 10.0, 20.0, 30.0, 40.0]);

    // Reorder, and move the last child to another parent.
    ui.prepend(list, nodes[1]);
    ui.append(other, nodes[3]);
    f.tree.mark_node_dirty(list);
    f.tree.mark_node_dirty(other);
    f.build(&ui, root);
    f.tree.clear_flags(input_root);

    assert_eq!(
        f.children(list_fragment),
        vec![keys[1], keys[0], inserted_key, keys[2]]
    );
    assert_eq!(tops(&f, list_fragment), vec![0.0, 10.0, 20.0, 30.0]);
    // Moving to another parent creates a new input item, and so a new box.
    assert_eq!(f.children(other_fragment), vec![fragment(&f, nodes[3])]);
    assert!(f.fragments.items.get(keys[3]).is_none());

    // Remove from the middle.
    ui.delete(inserted);
    f.tree.mark_node_dirty(list);
    f.build(&ui, root);

    assert_eq!(f.children(list_fragment), vec![keys[1], keys[0], keys[2]]);
    assert_eq!(tops(&f, list_fragment), vec![0.0, 10.0, 20.0]);
    assert!(f.fragments.items.get(inserted_key).is_none());
}

#[test]
fn removed_items_drop_their_fragments() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let block = sized_block(&mut ui, 20.0);
    let inner = text(&mut ui, "inner");
    ui.append(root, block);
    ui.append(block, inner);

    let mut f = Fixture::new();
    let (input_root, root_fragment) = f.build(&ui, root);
    f.tree.clear_flags(input_root);

    ui.delete(block);
    f.tree.mark_node_dirty(root);
    f.build(&ui, root);

    assert!(f.children(root_fragment).is_empty());
    assert_eq!(f.fragments.items.len(), 1);
    assert!(f.fragments.inline_layout(root_fragment).is_none());

    f.tree.delete_item(input_root);
    assert_eq!(
        f.builder.build(
            &ui,
            &f.tree,
            input_root,
            &mut f.fragments,
            &LayoutConstraint::new_root(VIEWPORT, 16.0),
        ),
        None
    );
    assert!(f.fragments.items.is_empty());
}

#[test]
fn atomic_inlines_are_placed_on_lines() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before ");
    let atomic = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    let inner = text(&mut ui, "badge");
    let after = text(&mut ui, " after");
    ui.append(root, before);
    ui.append(root, atomic);
    ui.append(atomic, inner);
    ui.append(root, after);

    let mut f = Fixture::new();
    let (_, root_fragment) = f.build(&ui, root);

    assert_eq!(f.items(root_fragment), vec![FragmentItem::Line { line: 0 }]);

    let line = f.children(root_fragment)[0];
    let atomic_fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(atomic).unwrap())
        .unwrap();

    assert_eq!(f.fragments.items.parent(atomic_fragment), Some(line));
    let atomic_box = f.fragments.items[atomic_fragment].layout;
    assert!(atomic_box.location.x > 0.0);
    assert!(atomic_box.size.width > 0.0);
    assert!(atomic_box.baseline.is_some());
    assert_eq!(
        f.items(atomic_fragment),
        vec![FragmentItem::Line { line: 0 }]
    );
}

#[test]
fn fragments_iterate_linked_fragments_of_an_item() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let before = text(&mut ui, "before");
    let block = sized_block(&mut ui, 20.0);
    let after = text(&mut ui, "after");
    ui.append(root, span);
    ui.append(span, before);
    ui.append(span, block);
    ui.append(span, after);

    let mut f = Fixture::new();
    let (input_root, root_fragment) = f.build(&ui, root);
    f.tree.clear_flags(input_root);

    let span_key = f.tree.item_for_node(span).unwrap();
    let spans: Vec<_> = f.fragments.fragments(span_key).collect();
    let children = f.children(root_fragment);

    assert_eq!(spans.len(), 2);
    assert_eq!(
        f.fragments.items.parent(spans[0]),
        Some(f.children(children[0])[0])
    );
    assert_eq!(
        f.fragments.items.parent(spans[1]),
        Some(f.children(children[2])[0])
    );
    assert!(
        spans
            .iter()
            .all(|&key| f.fragments.items[key].source == span_key)
    );

    let block_key = f.tree.item_for_node(block).unwrap();
    assert_eq!(
        f.fragments.fragments(block_key).collect::<Vec<_>>(),
        vec![children[1]]
    );
    assert_eq!(
        f.fragments.fragments(input_root).collect::<Vec<_>>(),
        vec![root_fragment]
    );

    let before_key = f.tree.item_for_node(before).unwrap();
    assert!(f.fragments.fragments(before_key).all(|key| matches!(
        f.fragments.items[key].item,
        FragmentItem::GlyphRun { line: 0, .. }
    )));

    // Removed items yield nothing.
    ui.delete(span);
    f.tree.mark_node_dirty(root);
    f.build(&ui, root);

    assert_eq!(f.fragments.fragments(span_key).count(), 0);
    assert_eq!(f.fragments.fragments(before_key).count(), 0);
    assert_eq!(f.fragments.fragments(block_key).count(), 0);
}

/// Horizontal extents of the glyph runs on each line of a box, relative to
/// the box, as `(line, left, right)`.
fn line_runs(f: &Fixture, node: NodeKey) -> Vec<(usize, f64, f64)> {
    let fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(node).unwrap())
        .unwrap();

    f.children(fragment)
        .into_iter()
        .filter_map(|line| match f.fragments.items[line].item {
            FragmentItem::Line { line: index } => Some((line, index)),
            _ => None,
        })
        .map(|(line, index)| {
            let offset = f.fragments.items[line].layout.location.x;
            let (left, right) = f
                .children(line)
                .into_iter()
                .map(|run| f.fragments.items[run].layout)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(left, right), run| {
                    (
                        left.min(run.location.x),
                        right.max(run.location.x + run.size.width),
                    )
                });

            (index, offset + left, offset + right)
        })
        .collect()
}

fn aligned_text(ui: &mut Ui, align: TextAlign, value: &str) -> NodeKey {
    let root = ui.create(Node::Box, (bx::TextAlign(align),));
    let content = text(ui, value);
    ui.append(root, content);

    root
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.5, "{actual} != {expected}");
}

#[test]
fn lines_are_aligned_within_the_content_box() {
    let mut ui = Ui::new();
    let start_node = aligned_text(&mut ui, TextAlign::Start, "short");
    let center_node = aligned_text(&mut ui, TextAlign::Center, "short");
    let right_node = aligned_text(&mut ui, TextAlign::Right, "short");

    let mut f = Fixture::new();
    f.build(&ui, start_node);
    let [(_, left, width)] = line_runs(&f, start_node)[..] else {
        panic!("expected one line");
    };
    let width = width - left;

    close(left, 0.0);

    f.build(&ui, center_node);
    let [(_, left, right)] = line_runs(&f, center_node)[..] else {
        panic!("expected one line");
    };
    close(left, (VIEWPORT.width - width) / 2.0);
    close(right - left, width);

    f.build(&ui, right_node);
    let [(_, left, right)] = line_runs(&f, right_node)[..] else {
        panic!("expected one line");
    };
    close(right, VIEWPORT.width);
    close(left, VIEWPORT.width - width);
}

#[test]
fn justified_lines_fill_the_width_except_the_last() {
    let value = "justified text spreads words across the whole line until \
                 the paragraph ends with a shorter last line";
    let mut ui = Ui::new();
    let start = aligned_text(&mut ui, TextAlign::Start, value);
    let justify = aligned_text(&mut ui, TextAlign::Justify, value);

    let mut f = Fixture::new();
    f.build(&ui, start);
    let start = line_runs(&f, start);
    f.build(&ui, justify);
    let justify = line_runs(&f, justify);

    assert!(start.len() > 1);
    assert_eq!(start.len(), justify.len());

    let (last, rest) = justify.split_last().unwrap();

    for (&(_, left, right), &(_, _, start_right)) in rest.iter().zip(&start) {
        close(left, 0.0);
        assert!(right > start_right, "{right} <= {start_right}");
    }

    // The last line keeps start alignment.
    assert_eq!(*last, *start.last().unwrap());
}

#[test]
fn text_align_is_inherited_and_updates_descendants() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (bx::TextAlign(TextAlign::Left),));
    let paragraph = ui.create(Node::Box, ());
    let content = text(&mut ui, "inherited");
    ui.append(root, paragraph);
    ui.append(paragraph, content);

    let mut f = Fixture::new();
    let (input_root, _) = f.build(&ui, root);
    f.tree.clear_flags(input_root);

    let [(_, left, right)] = line_runs(&f, paragraph)[..] else {
        panic!("expected one line");
    };
    close(left, 0.0);

    // Only the ancestor is marked; the change reaches the inline layout below.
    ui.set_props(root, (bx::TextAlign(TextAlign::Right),));
    f.tree.mark_node_dirty(root);
    f.build(&ui, root);

    let paragraph_key = f.tree.item_for_node(paragraph).unwrap();
    assert_eq!(
        f.tree.items[paragraph_key].text_style.align,
        TextAlign::Right
    );

    let [(_, aligned_left, aligned_right)] = line_runs(&f, paragraph)[..] else {
        panic!("expected one line");
    };
    close(aligned_right, VIEWPORT.width);
    close(aligned_right - aligned_left, right - left);
}

fn px(value: f64) -> AutoValue<StyleUnit> {
    AutoValue::Value(StyleUnit::Px(value))
}

fn fixed_size(width: f64, height: f64) -> bx::Size {
    bx::Size(Size::new(px(width), px(height)))
}

#[test]
fn absolute_boxes_are_positioned_by_their_positioned_parent() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let wrapper = ui.create(Node::Box, (fixed_size(100.0, 50.0),));
    let abs = ui.create(
        Node::Box,
        (
            bx::Position(Position::Absolute),
            bx::Inset(Rect::new_invidiual(
                px(5.0),
                px(0.0),
                AutoValue::Auto,
                AutoValue::Auto,
            )),
            fixed_size(10.0, 10.0),
        ),
    );
    ui.append(root, wrapper);
    ui.append(wrapper, abs);

    let mut f = Fixture::new();
    f.build(&ui, root);

    let wrapper_fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(wrapper).unwrap())
        .unwrap();
    let abs_fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(abs).unwrap())
        .unwrap();
    let layout = f.fragments.items[abs_fragment].layout;

    assert_eq!(f.fragments.items.parent(abs_fragment), Some(wrapper_fragment));
    // `right: 0` resolves against the 100px wrapper, not the 400px root.
    assert_eq!(layout.location, Point::new(90.0, 5.0));
    assert_eq!(layout.size, Size::new(10.0, 10.0));
}

#[test]
fn absolute_boxes_hoisted_past_static_parents_stay_parent_relative() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let wrapper = ui.create(
        Node::Box,
        (
            bx::Position(Position::Static),
            margin(30.0, 0.0),
            fixed_size(100.0, 50.0),
        ),
    );
    let abs = ui.create(
        Node::Box,
        (
            bx::Position(Position::Absolute),
            bx::Inset(Rect::new_invidiual(
                px(5.0),
                AutoValue::Auto,
                AutoValue::Auto,
                px(7.0),
            )),
            fixed_size(10.0, 10.0),
        ),
    );
    ui.append(root, wrapper);
    ui.append(wrapper, abs);

    let mut f = Fixture::new();
    let (root_key, _) = f.build(&ui, root);
    f.tree.clear_flags(root_key);

    let wrapper_fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(wrapper).unwrap())
        .unwrap();
    let abs_fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(abs).unwrap())
        .unwrap();

    assert_eq!(f.fragments.items.parent(abs_fragment), Some(wrapper_fragment));
    assert_eq!(
        f.fragments.items[wrapper_fragment].layout.location,
        Point::new(0.0, 30.0)
    );
    // The root contains the box at (7, 5); fragments stay relative to their
    // parent, the static wrapper 30px down.
    assert_eq!(
        f.fragments.items[abs_fragment].layout.location,
        Point::new(7.0, -25.0)
    );

    // A cached root layout still runs the root positioning pass.
    f.build(&ui, root);

    assert_eq!(
        f.fragments.items[abs_fragment].layout.location,
        Point::new(7.0, -25.0)
    );
}
