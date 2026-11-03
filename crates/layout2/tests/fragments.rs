use comet_layout2::{
    fragment::{FragmentItem, FragmentKey, builder::FragmentBuilder, tree::FragmentTree},
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Point, Rect, Size, StyleUnit},
    },
    taffy::conv,
    tree::{LayoutTree, builder::LayoutTreeBuilder},
};
use comet_tree::{
    node::{Node, Text},
    ui::Ui,
};

fn constraint(width: f64) -> LayoutConstraint {
    LayoutConstraint::new_root(Size::new(width, 600.0), 16.0)
}

fn keys(tree: &FragmentTree, root: FragmentKey) -> Vec<FragmentKey> {
    let mut result = Vec::new();
    let mut pending = vec![root];
    while let Some(key) = pending.pop() {
        result.push(key);
        pending.extend(tree.items.children(key).rev());
    }

    result
}

#[test]
fn conversions_preserve_percent_and_auto_and_edge_order() {
    let cx = constraint(500.0);
    assert_eq!(
        conv::length(StyleUnit::Percent(0.25), &cx),
        taffy::LengthPercentage::percent(0.25)
    );
    assert_eq!(
        conv::length(StyleUnit::Percent(0.5), &cx),
        taffy::LengthPercentage::percent(0.5)
    );
    assert_eq!(
        conv::length_auto(AutoValue::Auto, &cx),
        taffy::LengthPercentageAuto::auto()
    );
    assert_eq!(
        conv::length(StyleUnit::Em(2.0), &cx),
        taffy::LengthPercentage::length(32.0)
    );
    let rect = Rect::new_invidiual(1, 2, 3, 4);
    assert_eq!(conv::from_rect(conv::to_rect(rect, |v| v), |v| v), rect);
    let point = Point::new(3, 4);
    assert_eq!(conv::from_point(conv::to_point(point, |v| v), |v| v), point);
    let size = Size::new(8, 9);
    assert_eq!(conv::from_size(conv::to_size(size, |v| v), |v| v), size);
}

#[test]
fn percentages_use_actual_parent_and_box_properties_do_not_inherit() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::Size(Size::new(
                AutoValue::Value(StyleUnit::Px(200.0)),
                AutoValue::Auto,
            )),
            bx::Padding(Rect::new_axis(StyleUnit::Px(10.0), StyleUnit::Px(10.0))),
        ),
    );
    let child = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Percent(0.5)),
            AutoValue::Value(StyleUnit::Px(20.0)),
        )),),
    );
    ui.append(root, child);
    let mut layout = LayoutTree::new();
    let root = LayoutTreeBuilder::new()
        .build(&ui, root, &mut layout, None)
        .unwrap();
    let mut tree = FragmentTree::new();
    let root = FragmentBuilder::new()
        .build(&constraint(500.0), &ui, &layout, root, &mut tree)
        .unwrap();
    let child = tree.items.children(root).next().unwrap();
    assert_eq!(tree.items[root].layout.size.width, 200.0);
    assert_eq!(tree.items[child].layout.size.width, 90.0);
    assert_eq!(tree.items[child].layout.padding, Rect::default());
    assert_eq!(tree.items[child].layout.location, Point::new(10.0, 10.0));
}

#[test]
fn text_inherits_wraps_links_and_reconciles() {
    let mut ui = Ui::new();
    let root_node = ui.create(Node::Box, (bx::FontSize(StyleUnit::Px(24.0)),));
    let span = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::FontSize(StyleUnit::Em(0.5))),
    );
    let text = ui.create(
        Node::Text,
        (Text(
            "one two three four five six seven eight nine ten".into(),
        ),),
    );
    ui.append(root_node, span);
    ui.append(span, text);
    let mut layout = LayoutTree::new();
    let mut layout_builder = LayoutTreeBuilder::new();
    let root = layout_builder
        .build(&ui, root_node, &mut layout, None)
        .unwrap();
    let mut tree = FragmentTree::new();
    let mut builder = FragmentBuilder::new();
    let result = builder
        .build(&constraint(80.0), &ui, &layout, root, &mut tree)
        .unwrap();
    assert_eq!(tree.text_styles[&text].font_size, 12.0);
    let original = keys(&tree, result);
    let lines: Vec<_> = original
        .iter()
        .filter(|&&key| matches!(tree.items[key].item, FragmentItem::Line { .. }))
        .collect();
    assert!(lines.len() > 1);
    let spans: Vec<_> = original
        .iter()
        .copied()
        .filter(|&key| tree.items[key].node == Some(span))
        .collect();
    assert_eq!(spans.len(), lines.len());
    for pair in spans.windows(2) {
        assert_eq!(tree.items[pair[0]].link.next, Some(pair[1]));
        assert_eq!(tree.items[pair[1]].link.prev, Some(pair[0]));
    }

    assert_eq!(
        builder.build(&constraint(80.0), &ui, &layout, root, &mut tree),
        Some(result)
    );
    assert_eq!(keys(&tree, result), original);
    ui.set_props(root_node, (bx::FontSize(StyleUnit::Px(32.0)),));
    assert_eq!(
        builder.build(&constraint(80.0), &ui, &layout, root, &mut tree),
        Some(result)
    );
    assert_eq!(tree.text_styles[&text].font_size, 16.0);
    ui.set_props(text, (Text("short".into()),));
    builder.build(&constraint(500.0), &ui, &layout, root, &mut tree);
    assert!(tree.items.len() < original.len());

    ui.delete(span);
    layout_builder.build(&ui, root_node, &mut layout, Some(root));
    builder.build(&constraint(500.0), &ui, &layout, root, &mut tree);
    assert_eq!(tree.items.len(), 1);
    assert!(tree.inline_layouts.is_empty());
}

#[test]
fn atomic_inline_is_positioned_by_parley_and_keeps_its_subtree() {
    let mut ui = Ui::new();
    let root_node = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, (Text("before ".into()),));
    let atomic = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            bx::DisplayInner::FlowRoot,
            bx::Size(Size::new(
                AutoValue::Value(StyleUnit::Px(40.0)),
                AutoValue::Value(StyleUnit::Px(30.0)),
            )),
        ),
    );
    let child = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Auto,
            AutoValue::Value(StyleUnit::Px(10.0)),
        )),),
    );
    ui.append(root_node, text);
    ui.append(root_node, atomic);
    ui.append(atomic, child);
    let mut layout = LayoutTree::new();
    let root = LayoutTreeBuilder::new()
        .build(&ui, root_node, &mut layout, None)
        .unwrap();
    let mut tree = FragmentTree::new();
    let root = FragmentBuilder::new()
        .build(&constraint(200.0), &ui, &layout, root, &mut tree)
        .unwrap();
    let atomic_key = keys(&tree, root)
        .into_iter()
        .find(|&key| tree.items[key].node == Some(atomic))
        .unwrap();
    assert_eq!(tree.items[atomic_key].layout.size, Size::new(40.0, 30.0));
    assert!(tree.items[atomic_key].layout.location.x > 0.0);
    assert_eq!(tree.items.children(atomic_key).count(), 1);
}

#[test]
fn incremental_edits_match_fresh_layout_and_deleted_roots_are_pruned() {
    let mut ui = Ui::new();
    let root_node = ui.create(Node::Box, ());
    let mut texts = Vec::new();
    for _ in 0..3 {
        let block = ui.create(Node::Box, ());
        let text = ui.create(
            Node::Text,
            (Text("one two three four five six seven".into()),),
        );
        ui.append(root_node, block);
        ui.append(block, text);
        texts.push(text);
    }

    let mut layout = LayoutTree::new();
    let root = LayoutTreeBuilder::new()
        .build(&ui, root_node, &mut layout, None)
        .unwrap();
    let mut builder = FragmentBuilder::new();
    let mut tree = FragmentTree::new();
    for step in 0..6 {
        if step % 2 == 1 {
            ui.set_props(
                texts[1],
                (Text(format!("changed text {step} with more words")),),
            );
        }
        let cx = constraint(if step < 3 { 120.0 } else { 240.0 });
        let result = builder.build(&cx, &ui, &layout, root, &mut tree).unwrap();
        let mut fresh = FragmentTree::new();
        let fresh_root = FragmentBuilder::new()
            .build(&cx, &ui, &layout, root, &mut fresh)
            .unwrap();
        let actual = keys(&tree, result);
        let expected = keys(&fresh, fresh_root);
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.into_iter().zip(expected) {
            let a = &tree.items[a];
            let b = &fresh.items[b];
            assert_eq!((&a.item, a.node, a.layout), (&b.item, b.node, b.layout));
        }
    }

    layout.delete_item(root);
    assert_eq!(
        builder.build(&constraint(120.0), &ui, &layout, root, &mut tree),
        None
    );
    assert!(tree.items.is_empty());
    assert!(tree.inline_layouts.is_empty());
    assert!(tree.texts.is_empty());
}

#[test]
fn absolute_child_uses_taffy_containing_block() {
    let mut ui = Ui::new();
    let root_node = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Px(200.0)),
            AutoValue::Value(StyleUnit::Px(100.0)),
        )),),
    );
    let child = ui.create(
        Node::Box,
        (
            bx::Position(taffy::Position::Absolute),
            bx::Inset(Rect::new(
                Point::new(
                    AutoValue::Value(StyleUnit::Px(15.0)),
                    AutoValue::Value(StyleUnit::Px(20.0)),
                ),
                Point::default(),
            )),
            bx::Size(Size::new(
                AutoValue::Value(StyleUnit::Px(30.0)),
                AutoValue::Value(StyleUnit::Px(40.0)),
            )),
        ),
    );
    ui.append(root_node, child);
    let mut layout = LayoutTree::new();
    let root = LayoutTreeBuilder::new()
        .build(&ui, root_node, &mut layout, None)
        .unwrap();
    let mut tree = FragmentTree::new();
    let root = FragmentBuilder::new()
        .build(&constraint(500.0), &ui, &layout, root, &mut tree)
        .unwrap();
    let child = tree.items.children(root).next().unwrap();
    assert_eq!(tree.items[child].layout.location, Point::new(15.0, 20.0));
    assert_eq!(tree.items[child].layout.size, Size::new(30.0, 40.0));
}

#[test]
fn empty_inline_continuations_keep_fragment_links() {
    let mut ui = Ui::new();
    let root_node = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let block = ui.create(Node::Box, ());
    ui.append(root_node, span);
    ui.append(span, block);

    let mut layout = LayoutTree::new();
    let root = LayoutTreeBuilder::new()
        .build(&ui, root_node, &mut layout, None)
        .unwrap();
    let mut tree = FragmentTree::new();
    let root = FragmentBuilder::new()
        .build(&constraint(200.0), &ui, &layout, root, &mut tree)
        .unwrap();
    let spans: Vec<_> = keys(&tree, root)
        .into_iter()
        .filter(|&key| tree.items[key].node == Some(span))
        .collect();
    assert_eq!(spans.len(), 2);
    assert_eq!(tree.items[spans[0]].link.next, Some(spans[1]));
    assert_eq!(tree.items[spans[1]].link.prev, Some(spans[0]));
}

#[test]
fn text_percentages_use_inherited_font_size_and_computed_line_height() {
    let mut ui = Ui::new();
    let parent = ui.create(Node::Box, (bx::FontSize(StyleUnit::Px(20.0)),));
    let child = ui.create(
        Node::Box,
        (
            bx::FontSize(StyleUnit::Percent(1.5)),
            bx::LineHeight(StyleUnit::Percent(2.0)),
        ),
    );
    let text = ui.create(Node::Text, (bx::FontSize(StyleUnit::Percent(0.5)),));
    ui.append(parent, child);
    ui.append(child, text);
    let mut cache = Default::default();
    let style = comet_layout2::parley::style::resolve(&ui, text, &constraint(500.0), &mut cache);

    assert_eq!(cache[&child].font_size, 30.0);
    assert_eq!(style.font_size, 15.0);
    assert_eq!(style.line_height, parley::LineHeight::Absolute(60.0));
}

#[test]
fn one_percentage_unit_uses_each_taffy_axis() {
    let mut ui = Ui::new();
    let root_node = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Px(200.0)),
            AutoValue::Value(StyleUnit::Px(100.0)),
        )),),
    );
    let child = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Percent(0.5)),
            AutoValue::Value(StyleUnit::Percent(0.5)),
        )),),
    );
    ui.append(root_node, child);
    let mut layout = LayoutTree::new();
    let root = LayoutTreeBuilder::new()
        .build(&ui, root_node, &mut layout, None)
        .unwrap();
    let mut tree = FragmentTree::new();
    let root = FragmentBuilder::new()
        .build(&constraint(500.0), &ui, &layout, root, &mut tree)
        .unwrap();
    let child = tree.items.children(root).next().unwrap();

    assert_eq!(tree.items[child].layout.size, Size::new(100.0, 50.0));
}
