use comet_layout::{
    fragment::{FragmentItem, FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        LayoutFlags, LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Size, StyleUnit},
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

#[test]
fn block_children_split_inline_content() {
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

    assert_eq!(
        f.items(root_fragment),
        vec![
            FragmentItem::Line { line: 0 },
            FragmentItem::Box,
            FragmentItem::Line { line: 2 },
        ]
    );

    let children = f.children(root_fragment);
    let first = f.fragments.items[children[0]].layout;
    let block_box = f.fragments.items[children[1]].layout;
    let second = f.fragments.items[children[2]].layout;

    assert!(first.size.height > 0.0);
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
    assert!(f.fragments.inline_layouts.get(input_root).is_some());
}

#[test]
fn leading_and_trailing_blocks_add_no_empty_lines() {
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
            FragmentItem::Line { line: 1 },
            FragmentItem::Box,
        ]
    );

    let children = f.children(root_fragment);
    let line = f.fragments.items[children[1]].layout;
    let last = f.fragments.items[children[2]].layout;

    assert_eq!(f.fragments.items[children[0]].layout.location.y, 0.0);
    assert_eq!(line.location.y, 10.0);
    assert_eq!(last.location.y, line.location.y + line.size.height);
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

    let first_span = f.children(children[0])[0];
    let second_span = f.children(children[2])[0];
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
    assert!(f.fragments.inline_layouts.is_empty());

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
    assert_eq!(f.fragments.items.parent(spans[0]), Some(children[0]));
    assert_eq!(f.fragments.items.parent(spans[1]), Some(children[2]));
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
