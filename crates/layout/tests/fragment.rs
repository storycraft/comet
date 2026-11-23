use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{FragmentItem, FragmentKey, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Point, Rect, Size, StyleUnit},
        parley::Alignment as TextAlign,
        taffy::Position,
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

    fn build(&mut self, ui: &Ui, node: NodeKey) -> (LayoutInputKey, FragmentKey) {
        self.build_with(ui, node, VIEWPORT)
    }

    fn build_with(
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

        (root, self.layout_with(ui, root, viewport))
    }

    /// Lay out the input tree as marked, without building it again.
    fn layout(&mut self, ui: &Ui, root: LayoutInputKey) -> FragmentKey {
        self.layout_with(ui, root, VIEWPORT)
    }

    fn layout_with(&mut self, ui: &Ui, root: LayoutInputKey, viewport: Size<f64>) -> FragmentKey {
        self.builder
            .build(
                ui,
                &self.tree,
                &mut self.dirty,
                root,
                &mut self.fragments,
                &LayoutConstraint::new_root(viewport, 16.0),
            )
            .unwrap()
    }

    fn box_of(&self, node: NodeKey) -> FragmentKey {
        self.fragments
            .box_fragment(self.tree.item_for_node(node).unwrap())
            .unwrap()
    }

    /// The source text of the inline layout owned by a fragment.
    fn shaped_text(&self, key: FragmentKey) -> String {
        self.fragments.inline_layout(key).unwrap().text.clone()
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

    assert!(matches!(
        f.items(root_fragment).as_slice(),
        [
            FragmentItem::AnonymousBlock { .. },
            FragmentItem::Box { .. },
            FragmentItem::AnonymousBlock { .. },
        ]
    ));

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

    assert!(matches!(
        f.items(root_fragment).as_slice(),
        [
            FragmentItem::Box { .. },
            FragmentItem::AnonymousBlock { .. },
            FragmentItem::Box { .. },
        ]
    ));

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

    assert!(matches!(
        f.items(root_fragment).as_slice(),
        [
            FragmentItem::AnonymousBlock { .. },
            FragmentItem::Box { .. },
            FragmentItem::Box { .. },
            FragmentItem::AnonymousBlock { .. },
        ]
    ));

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
fn anonymous_blocks_are_found_by_the_block_their_run_follows() {
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
    let owner = f.tree.item_for_node(root).unwrap();
    let block_item = f.tree.item_for_node(block).unwrap();
    let run_text = |f: &Fixture, after| {
        let anonymous = f.fragments.anonymous_block(owner, after)?;
        assert_eq!(f.fragments.run_after(anonymous), Some(after));
        Some(f.shaped_text(anonymous))
    };

    assert_eq!(run_text(&f, None).as_deref(), Some("before"));
    assert_eq!(run_text(&f, Some(block_item)).as_deref(), Some("after"));
    assert_eq!(f.fragments.run_after(root_fragment), None, "not a run");

    // A new block splits the first run. Anonymous blocks are reused in
    // order, but each run is still found by the block it follows.
    let middle = text(&mut ui, "middle");
    let split = sized_block(&mut ui, 20.0);
    ui.after(before, split);
    ui.after(split, middle);
    f.dirty.mark_node(&f.tree, root, DirtyFlags::NEED_CHILDREN);
    f.build(&ui, root);
    let split_item = f.tree.item_for_node(split).unwrap();

    assert_eq!(run_text(&f, None).as_deref(), Some("before"));
    assert_eq!(run_text(&f, Some(split_item)).as_deref(), Some("middle"));
    assert_eq!(run_text(&f, Some(block_item)).as_deref(), Some("after"));

    // Without block boxes there are no runs.
    ui.delete(split);
    ui.delete(block);
    f.dirty.mark_node(&f.tree, root, DirtyFlags::NEED_CHILDREN);
    f.build(&ui, root);
    assert_eq!(f.fragments.anonymous_block(owner, None), None);
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
    f.dirty.clear(&f.tree, input_root);

    let children = f.children(root_fragment);
    let first_lines = f.children(children[0]);
    let second_lines = f.children(children[2]);

    ui.set_props(after, (Text("after, but longer".into()),));
    f.dirty
        .mark_node(&f.tree, after, DirtyFlags::NEED_FULL_LAYOUT);
    f.layout(&ui, input_root);
    f.dirty.clear(&f.tree, input_root);

    // Anonymous blocks are reused, and only the edited run is emitted again.
    assert_eq!(f.children(root_fragment), children);
    assert_eq!(f.children(children[0]), first_lines);
    assert_ne!(f.children(children[2]), second_lines);

    // Removing the block merges the runs into one inline formatting context.
    ui.delete(block);
    f.dirty.mark_node(&f.tree, root, DirtyFlags::NEED_CHILDREN);
    f.build(&ui, root);

    assert_eq!(f.items(root_fragment), vec![FragmentItem::Line { line: 0 }]);
    assert!(f.fragments.items.get(children[0]).is_none());
    assert!(f.fragments.items.get(children[2]).is_none());
}

#[test]
fn paint_only_flags_keep_layouts() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let header = sized_block(&mut ui, 40.0);
    let body = ui.create(Node::Box, ());
    let content = text(&mut ui, "body");
    let mixed = ui.create(Node::Box, ());
    let before = text(&mut ui, "before");
    let block = sized_block(&mut ui, 20.0);
    ui.append(root, header);
    ui.append(root, body);
    ui.append(body, content);
    ui.append(root, mixed);
    ui.append(mixed, before);
    ui.append(mixed, block);

    let mut f = Fixture::new();
    let (input_root, _) = f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);

    let body_lines = f.children(f.box_of(body));
    let runs = f.children(f.box_of(mixed));
    let run_lines = f.children(runs[0]);

    // A paint change on a box, and on text in a run, next to layout changes
    // reaching them: the header moves the body, and the block lays out the
    // runs of its owner again.
    f.dirty.mark_node(&f.tree, body, DirtyFlags::NEED_PRE_PAINT);
    f.dirty
        .mark_node(&f.tree, before, DirtyFlags::NEED_PRE_PAINT);
    ui.set_props(
        header,
        (bx::Size(Size::new(
            AutoValue::Auto,
            AutoValue::Value(StyleUnit::Px(60.0)),
        )),),
    );
    f.dirty
        .mark_node(&f.tree, header, DirtyFlags::NEED_BOX_LAYOUT);
    ui.set_props(
        block,
        (bx::Size(Size::new(
            AutoValue::Auto,
            AutoValue::Value(StyleUnit::Px(30.0)),
        )),),
    );
    f.dirty
        .mark_node(&f.tree, block, DirtyFlags::NEED_BOX_LAYOUT);
    f.layout(&ui, input_root);

    // Their lines are not laid out again.
    assert_eq!(f.children(f.box_of(body)), body_lines);
    assert_eq!(f.children(f.box_of(mixed))[0], runs[0]);
    assert_eq!(f.children(runs[0]), run_lines);
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
    f.dirty.clear(&f.tree, input_root);

    let body_key = f.tree.item_for_node(body).unwrap();
    let header_fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(header).unwrap());
    let body_fragment = f.fragments.box_fragment(body_key).unwrap();
    let lines = f.children(body_fragment);
    let count = f.fragments.items.len();

    // Clean trees keep every fragment.
    assert_eq!(f.layout(&ui, input_root), root_fragment);
    assert_eq!(f.children(body_fragment), lines);
    assert_eq!(f.fragments.items.len(), count);

    let short_height = f.fragments.items[body_fragment].layout.size.height;

    ui.set_props(
        content,
        (Text("a much longer text that has to wrap over several lines because the viewport is only four hundred pixels wide".into()),),
    );
    f.dirty
        .mark_node(&f.tree, content, DirtyFlags::NEED_FULL_LAYOUT);

    assert_eq!(f.build(&ui, root), (input_root, root_fragment));
    assert_eq!(
        f.fragments
            .box_fragment(f.tree.item_for_node(header).unwrap()),
        header_fragment
    );
    assert_eq!(f.fragments.box_fragment(body_key), Some(body_fragment));
    assert!(f.children(body_fragment).len() > 1);
    assert!(f.fragments.items[body_fragment].layout.size.height > short_height);

    f.dirty.clear(&f.tree, input_root);
    assert!(
        f.tree
            .items
            .descendants(input_root)
            .all(|key| f.dirty.entry(key).flags.is_empty())
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
    f.dirty.clear(&f.tree, input_root);

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
    f.dirty.mark_node(&f.tree, list, DirtyFlags::NEED_CHILDREN);
    f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);

    let inserted_key = fragment(&f, inserted);
    assert_eq!(
        f.children(list_fragment),
        vec![keys[0], keys[1], inserted_key, keys[2], keys[3]]
    );
    assert_eq!(tops(&f, list_fragment), vec![0.0, 10.0, 20.0, 30.0, 40.0]);

    // Reorder, and move the last child to another parent.
    ui.prepend(list, nodes[1]);
    ui.append(other, nodes[3]);
    f.dirty.mark_node(&f.tree, list, DirtyFlags::NEED_CHILDREN);
    f.dirty.mark_node(&f.tree, other, DirtyFlags::NEED_CHILDREN);
    f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);

    // Every child keeps its box fragment, in the new order.
    assert_eq!(fragment(&f, nodes[1]), keys[1]);
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
    f.dirty.mark_node(&f.tree, list, DirtyFlags::NEED_CHILDREN);
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
    f.dirty.clear(&f.tree, input_root);

    ui.delete(block);
    f.dirty.mark_node(&f.tree, root, DirtyFlags::NEED_CHILDREN);
    f.build(&ui, root);

    assert!(f.children(root_fragment).is_empty());
    assert_eq!(f.fragments.items.len(), 1);
    assert!(f.fragments.inline_layout(root_fragment).is_none());

    f.tree.delete_item(input_root);
    assert_eq!(
        f.builder.build(
            &ui,
            &f.tree,
            &mut f.dirty,
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
    f.dirty.clear(&f.tree, input_root);

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
    f.dirty.mark_node(&f.tree, root, DirtyFlags::NEED_CHILDREN);
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
    f.dirty.clear(&f.tree, input_root);

    let [(_, left, right)] = line_runs(&f, paragraph)[..] else {
        panic!("expected one line");
    };
    close(left, 0.0);

    // Only the ancestor is marked; the change reaches the inline layout below.
    ui.set_props(root, (bx::TextAlign(TextAlign::Right),));
    f.dirty
        .mark_node(&f.tree, root, DirtyFlags::NEED_FULL_LAYOUT);
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

    assert_eq!(
        f.fragments.items.parent(abs_fragment),
        Some(wrapper_fragment)
    );
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
    f.dirty.clear(&f.tree, root_key);

    let wrapper_fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(wrapper).unwrap())
        .unwrap();
    let abs_fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(abs).unwrap())
        .unwrap();

    assert_eq!(
        f.fragments.items.parent(abs_fragment),
        Some(wrapper_fragment)
    );
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

fn atomic(ui: &mut Ui, width: f64) -> NodeKey {
    ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            bx::DisplayInner::FlowRoot,
            fixed_size(width, 10.0),
        ),
    )
}

// Box dirty tests edit text without marking it: stale shaped text shows that
// the content was not shaped again, while geometry still follows the box.

#[test]
fn box_style_changes_keep_shaped_text() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let paragraph = ui.create(Node::Box, ());
    let content = text(&mut ui, "hello");
    ui.append(root, paragraph);
    ui.append(paragraph, content);

    let mut f = Fixture::new();
    let (input_root, _) = f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);

    ui.set_props(content, (Text("changed".into()),));
    ui.set_props(
        paragraph,
        (bx::Padding(Rect::new_all(StyleUnit::Px(10.0))),),
    );
    f.dirty
        .mark_node(&f.tree, paragraph, DirtyFlags::NEED_BOX_LAYOUT);
    f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);

    let paragraph_fragment = f.box_of(paragraph);
    let line = f.children(paragraph_fragment)[0];

    assert_eq!(f.shaped_text(paragraph_fragment), "hello");
    assert_eq!(
        f.fragments.items[line].layout.location,
        Point::new(10.0, 10.0)
    );

    // Content changes shape the text again.
    f.dirty
        .mark_node(&f.tree, content, DirtyFlags::NEED_FULL_LAYOUT);
    f.build(&ui, root);

    assert_eq!(f.shaped_text(paragraph_fragment), "changed");
}

#[test]
fn atomic_inline_box_changes_keep_the_paragraph_shaped() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "before ");
    let badge = atomic(&mut ui, 10.0);
    let after = text(&mut ui, " after");
    ui.append(root, before);
    ui.append(root, badge);
    ui.append(root, after);

    let mut f = Fixture::new();
    let (input_root, root_fragment) = f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);
    let shaped = f.shaped_text(root_fragment);

    ui.set_props(before, (Text("edited ".into()),));
    ui.set_props(badge, (fixed_size(30.0, 10.0),));
    f.dirty
        .mark_node(&f.tree, badge, DirtyFlags::NEED_BOX_LAYOUT);
    f.build(&ui, root);

    assert_eq!(f.shaped_text(root_fragment), shaped);
    assert_eq!(
        f.fragments.items[f.box_of(badge)].layout.size,
        Size::new(30.0, 10.0)
    );
    assert_eq!(
        f.fragments
            .items
            .parent(f.box_of(badge))
            .map(|key| f.fragments.items[key].item.clone()),
        Some(FragmentItem::Line { line: 0 })
    );
}

#[test]
fn atomic_inline_box_changes_lay_out_only_their_run() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let first = text(&mut ui, "first");
    let block = sized_block(&mut ui, 20.0);
    let second = text(&mut ui, "second ");
    let badge = atomic(&mut ui, 10.0);
    ui.append(root, first);
    ui.append(root, block);
    ui.append(root, second);
    ui.append(root, badge);

    let mut f = Fixture::new();
    let (input_root, root_fragment) = f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);

    let runs = f.children(root_fragment);
    let first_lines = f.children(runs[0]);
    let shaped = f.shaped_text(runs[2]);

    ui.set_props(second, (Text("edited ".into()),));
    ui.set_props(badge, (fixed_size(30.0, 10.0),));
    f.dirty
        .mark_node(&f.tree, badge, DirtyFlags::NEED_BOX_LAYOUT);
    f.layout(&ui, input_root);

    // The run is laid out again without shaping; the other run is untouched.
    assert_eq!(f.children(root_fragment), runs);
    assert_eq!(f.children(runs[0]), first_lines);
    assert_eq!(f.shaped_text(runs[2]), shaped);
    assert_eq!(
        f.fragments.items[f.box_of(badge)].layout.size,
        Size::new(30.0, 10.0)
    );
}

#[test]
fn rebuilds_break_lines_again_without_shaping() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let content = text(&mut ui, "several words that wrap at narrow widths");
    ui.append(root, content);

    let mut f = Fixture::new();
    let (input_root, root_fragment) = f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);
    let shaped = f.shaped_text(root_fragment);
    assert_eq!(f.children(root_fragment).len(), 1);

    // A viewport change keeps the shaped text of equal text styles, while
    // lines follow the new width.
    ui.set_props(content, (Text("edited".into()),));
    f.build_with(&ui, root, Size::new(80.0, 600.0));
    f.dirty.clear(&f.tree, input_root);

    assert_eq!(f.shaped_text(root_fragment), shaped);
    assert!(f.children(root_fragment).len() > 1);
    assert_eq!(f.fragments.items[root_fragment].layout.size.width, 80.0);
}

/// Kinds of the fragments under `key`, in tree order, comparable across
/// fragment trees.
fn item_kinds(f: &Fixture, key: FragmentKey) -> Vec<&'static str> {
    f.items(key)
        .into_iter()
        .map(|item| match item {
            FragmentItem::Box { .. } => "box",
            FragmentItem::AnonymousBlock { .. } => "anonymous",
            FragmentItem::Line { .. } => "line",
            FragmentItem::GlyphRun { .. } => "glyphs",
            FragmentItem::Inline { .. } => "inline",
        })
        .collect()
}

/// Re-layout `root` after marking `node` restyled, and check that the
/// children of `parent` match a fresh layout.
fn restyle_matches_fresh(f: &mut Fixture, ui: &Ui, root: NodeKey, node: NodeKey, parent: NodeKey) {
    f.dirty
        .mark_node(&f.tree, node, DirtyFlags::NEED_FULL_LAYOUT);
    let (input_root, _) = f.build(ui, root);
    f.dirty.clear(&f.tree, input_root);

    let mut fresh = Fixture::new();
    fresh.build(ui, root);

    assert_eq!(
        item_kinds(f, f.box_of(parent)),
        item_kinds(&fresh, fresh.box_of(parent))
    );
}

#[test]
fn children_changing_kind_change_their_parents_mode() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let block = sized_block(&mut ui, 20.0);
    let changing = ui.create(Node::Box, ());
    let content = text(&mut ui, "changing");
    ui.append(root, block);
    ui.append(root, changing);
    ui.append(changing, content);

    let mut f = Fixture::new();
    let (input_root, root_fragment) = f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);
    assert_eq!(item_kinds(&f, root_fragment), vec!["box", "box"]);

    // A block child becoming inline makes its parent's content mixed.
    ui.set_props(changing, (bx::DisplayOuter::Inline,));
    restyle_matches_fresh(&mut f, &ui, root, changing, root);
    assert_eq!(item_kinds(&f, root_fragment), vec!["box", "anonymous"]);

    // And back to block boxes only.
    ui.set_props(changing, (bx::DisplayOuter::Block,));
    restyle_matches_fresh(&mut f, &ui, root, changing, root);
    assert_eq!(item_kinds(&f, root_fragment), vec!["box", "box"]);
}

#[test]
fn nested_inline_content_changing_kind_changes_its_owners_mode() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let paragraph = ui.create(Node::Box, ());
    let before = text(&mut ui, "before ");
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let nested = ui.create(Node::Box, ());
    let inside = text(&mut ui, "inside");
    ui.append(root, paragraph);
    ui.append(paragraph, before);
    ui.append(paragraph, span);
    ui.append(span, nested);
    ui.append(nested, inside);

    let mut f = Fixture::new();
    let (input_root, _) = f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);
    // The block inside the span interrupts the paragraph's inline content.
    assert_eq!(
        item_kinds(&f, f.box_of(paragraph)),
        vec!["anonymous", "box"]
    );

    // As an inline element, nothing interrupts it: the paragraph holds lines.
    ui.set_props(nested, (bx::DisplayOuter::Inline,));
    restyle_matches_fresh(&mut f, &ui, root, nested, paragraph);
    assert_eq!(item_kinds(&f, f.box_of(paragraph)), vec!["line"]);

    // Inserting a block into the span interrupts it again.
    let interruption = sized_block(&mut ui, 10.0);
    ui.append(span, interruption);
    f.dirty.mark_node(&f.tree, span, DirtyFlags::NEED_CHILDREN);
    let (input_root, _) = f.build(&ui, root);
    f.dirty.clear(&f.tree, input_root);
    let mut fresh = Fixture::new();
    fresh.build(&ui, root);
    assert_eq!(
        item_kinds(&f, f.box_of(paragraph)),
        item_kinds(&fresh, fresh.box_of(paragraph))
    );
    assert!(item_kinds(&f, f.box_of(paragraph)).contains(&"box"));
}

fn overflow_of(f: &Fixture, node: NodeKey) -> kurbo::Rect {
    f.fragments.items[f.box_of(node)].layout.overflow
}

fn hidden() -> bx::Overflow {
    bx::Overflow(Point::new(
        comet_layout::style::taffy::Overflow::Hidden,
        comet_layout::style::taffy::Overflow::Hidden,
    ))
}

#[test]
fn overflow_covers_descendants_in_every_direction() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let parent = ui.create(Node::Box, (fixed_size(100.0, 50.0),));
    // Shifted up and left, and wider than the parent.
    let child = ui.create(
        Node::Box,
        (
            bx::Inset(Rect::new_invidiual(
                px(-10.0),
                AutoValue::Auto,
                AutoValue::Auto,
                px(-20.0),
            )),
            fixed_size(200.0, 30.0),
        ),
    );
    // Overflows the child downwards.
    let grandchild = ui.create(Node::Box, (fixed_size(10.0, 300.0),));
    ui.append(root, parent);
    ui.append(parent, child);
    ui.append(child, grandchild);

    let mut f = Fixture::new();
    f.build(&ui, root);

    assert_eq!(
        overflow_of(&f, grandchild),
        kurbo::Rect::new(0.0, 0.0, 10.0, 300.0)
    );
    assert_eq!(
        overflow_of(&f, child),
        kurbo::Rect::new(0.0, 0.0, 200.0, 300.0)
    );
    assert_eq!(
        overflow_of(&f, parent),
        kurbo::Rect::new(-20.0, -10.0, 180.0, 290.0)
    );
    // The root follows its overflowing descendants too.
    assert_eq!(overflow_of(&f, root).y1, 290.0);
}

#[test]
fn clipping_boxes_contribute_only_their_border_box() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let parent = ui.create(Node::Box, (fixed_size(100.0, 50.0),));
    let clipper = ui.create(Node::Box, (fixed_size(40.0, 20.0), hidden()));
    let content = ui.create(Node::Box, (fixed_size(10.0, 300.0),));
    ui.append(root, parent);
    ui.append(parent, clipper);
    ui.append(clipper, content);

    let mut f = Fixture::new();
    f.build(&ui, root);

    // The clipper covers its content, which its parent does not see.
    assert_eq!(
        overflow_of(&f, clipper),
        kurbo::Rect::new(0.0, 0.0, 40.0, 300.0)
    );
    assert_eq!(
        overflow_of(&f, parent),
        kurbo::Rect::new(0.0, 0.0, 100.0, 50.0)
    );
}

#[test]
fn hoisted_boxes_extend_the_overflow_of_the_boxes_between() {
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

    // Placed at (7, -25) in the static wrapper, above its border box.
    assert_eq!(
        overflow_of(&f, wrapper),
        kurbo::Rect::new(0.0, -25.0, 100.0, 50.0)
    );

    // A cached root layout keeps it.
    f.dirty.clear(&f.tree, root_key);
    f.build(&ui, root);

    assert_eq!(
        overflow_of(&f, wrapper),
        kurbo::Rect::new(0.0, -25.0, 100.0, 50.0)
    );
}

#[test]
fn inline_content_extends_the_overflow_with_its_atomic_inlines() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let paragraph = ui.create(Node::Box, (fixed_size(200.0, 20.0),));
    let inline_block = atomic(&mut ui, 20.0);
    // Overflows the 10px tall atomic inline.
    let tall = ui.create(Node::Box, (fixed_size(10.0, 100.0),));
    ui.append(root, paragraph);
    let label = text(&mut ui, "a ");
    ui.append(paragraph, label);
    ui.append(paragraph, inline_block);
    ui.append(inline_block, tall);

    let mut f = Fixture::new();
    f.build(&ui, root);

    let atomic_fragment = f.box_of(inline_block);
    assert_eq!(
        f.fragments.items[atomic_fragment].layout.overflow,
        kurbo::Rect::new(0.0, 0.0, 20.0, 100.0)
    );

    // Every fragment between the paragraph and the atomic inline covers it.
    let mut extent = f.fragments.items[atomic_fragment].layout.extent();
    let mut ancestor = f.fragments.items.parent(atomic_fragment);
    let paragraph_fragment = f.box_of(paragraph);

    while let Some(key) = ancestor {
        let layout = &f.fragments.items[key].layout;
        assert!(
            layout.overflow.contains_rect(extent),
            "{:?} does not cover {extent:?}",
            f.fragments.items[key].item
        );

        if key == paragraph_fragment {
            break;
        }

        extent = layout.extent();
        ancestor = f.fragments.items.parent(key);
    }

    assert!(overflow_of(&f, paragraph).height() > 100.0);
    // Taffy passes the extended overflow up to the root.
    assert!(overflow_of(&f, root).height() > 100.0);
}
