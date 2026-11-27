use comet_content::manager::ContentManager;
use std::ops::Deref;

use comet_geometry::{AutoValue, Rect, Size, StyleUnit};
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{
        BoxFragmentKey, FragmentGeometry, builder::FragmentTreeBuilder, tree::FragmentTree,
    },
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{LayoutConstraint, bx},
};
use comet_layout_flex::{flex as flex_layout, style as flex};
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

const VIEWPORT: Size<f64> = Size::new(800.0, 600.0);

fn size(width: f64, height: f64) -> bx::Size {
    bx::Size(Size::new(
        AutoValue::Value(StyleUnit::Px(width)),
        AutoValue::Value(StyleUnit::Px(height)),
    ))
}

/// Input and fragment trees of one root, built together.
#[derive(Default)]
struct Layout {
    inputs: LayoutInputTree,
    dirty: DirtyMap,
    fragments: FragmentTree,
    input_builder: LayoutInputTreeBuilder,
    builder: FragmentTreeBuilder,
}

impl Deref for Layout {
    type Target = FragmentTree;

    fn deref(&self) -> &FragmentTree {
        &self.fragments
    }
}

impl Layout {
    /// Build or update the trees of `root`, then clear the input flags.
    fn build(&mut self, ui: &Ui, root: NodeKey) -> BoxFragmentKey {
        let key = self
            .input_builder
            .build(
                ui,
                &mut self.inputs,
                &mut self.dirty,
                BuildRequest::new(root, 16.0, VIEWPORT),
            )
            .unwrap();
        let fragment = self
            .builder
            .build(
                ui,
                &self.inputs,
                &ContentManager::new(),
                &mut self.dirty,
                key,
                &mut self.fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0, 1.0),
            )
            .unwrap();
        self.dirty.clear(&self.inputs, key);

        fragment
    }

    fn item(&self, node: NodeKey) -> LayoutInputKey {
        self.inputs.item_for_node(node).unwrap()
    }

    fn geometry(&self, key: BoxFragmentKey) -> FragmentGeometry {
        self.fragments.boxes[key].layout.geometry
    }

    /// Whether the box `key` holds glyph runs of the text node `source`.
    fn has_glyphs_of(&self, key: BoxFragmentKey, source: NodeKey) -> bool {
        let source = self.item(source);

        self.fragments.inline_layout(key).is_some_and(|inline| {
            inline
                .items
                .iter()
                .any(|item| item.is_run() && item.source == source)
        })
    }

    /// The first baseline of the glyph runs of the text node `node`, from
    /// the border box top of `root`.
    fn first_baseline(&self, root: BoxFragmentKey, node: NodeKey) -> f64 {
        let source = self.item(node);
        let (owner, item) = self
            .fragments
            .boxes
            .descendants(root)
            .find_map(|key| {
                let item = self
                    .fragments
                    .inline_layout(key)?
                    .items
                    .iter()
                    .find(|item| item.is_run() && item.source == source)?;

                Some((key, item))
            })
            .unwrap();

        // Inline items are relative to the border box of their owner.
        let mut y = item.geometry.location.y + f64::from(item.baseline);
        let mut current = owner;

        while current != root {
            y += self.geometry(current).location.y;
            current = self.fragments.boxes.parent(current).unwrap();
        }

        y
    }
}

fn layout(ui: &Ui, root: NodeKey) -> (Layout, BoxFragmentKey) {
    let mut layout = Layout::default();
    let key = layout.build(ui, root);

    (layout, key)
}

fn box_for(tree: &Layout, root: BoxFragmentKey, node: NodeKey) -> FragmentGeometry {
    let key = tree.box_fragment(tree.item(node)).unwrap();

    assert!(tree.boxes.descendants(root).any(|other| other == key));
    assert!(!tree.is_anonymous(key));

    tree.geometry(key)
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.01, "{actual} != {expected}");
}

#[test]
fn grows_and_shrinks_items_with_gaps() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            size(300.0, 100.0),
            flex::Gap(Size::new(StyleUnit::Px(10.0), StyleUnit::ZERO)),
        ),
    );
    let a = ui.create(Node::Box, (size(50.0, 20.0), flex::FlexGrow(1.0)));
    let b = ui.create(Node::Box, (size(50.0, 20.0), flex::FlexGrow(2.0)));

    ui.append(root, a);
    ui.append(root, b);

    let (tree, key) = layout(&ui, root);
    let first = box_for(&tree, key, a);
    let second = box_for(&tree, key, b);

    close(first.size.width, 50.0 + 190.0 / 3.0);
    close(second.size.width, 50.0 + 380.0 / 3.0);
    close(second.location.x, first.size.width + 10.0);

    ui.set_props(root, (size(70.0, 100.0),));

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, a).size.width, 30.0);
    close(box_for(&tree, key, b).location.x, 40.0);
    close(box_for(&tree, key, b).size.width, 30.0);
}

#[test]
fn percentages_use_content_size_and_padding_is_applied_once() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            size(224.0, 100.0),
            bx::Padding(Rect::new_all(StyleUnit::Px(10.0))),
            bx::Border(Rect::new_all(StyleUnit::Px(2.0))),
            flex::Gap(Size::new(StyleUnit::Percent(0.1), StyleUnit::ZERO)),
        ),
    );
    let a = ui.create(
        Node::Box,
        (
            size(1.0, 20.0),
            flex::FlexBasis(AutoValue::Value(StyleUnit::Percent(0.25))),
        ),
    );
    let b = ui.create(
        Node::Box,
        (
            size(1.0, 20.0),
            flex::FlexBasis(AutoValue::Value(StyleUnit::Percent(0.25))),
        ),
    );

    ui.append(root, a);
    ui.append(root, b);

    for (box_sizing, width, height) in [
        (taffy::BoxSizing::BorderBox, 224.0, 100.0),
        (taffy::BoxSizing::ContentBox, 200.0, 76.0),
    ] {
        ui.set_props(root, (bx::BoxSizing(box_sizing), size(width, height)));

        let (tree, key) = layout(&ui, root);
        let a = box_for(&tree, key, a);
        let b = box_for(&tree, key, b);

        close(tree.geometry(key).size.width, 224.0);
        close(tree.geometry(key).size.height, 100.0);
        close(a.location.x, 12.0);
        close(a.location.y, 12.0);
        close(a.size.width, 50.0);
        close(b.location.x, 82.0);
        close(b.size.width, 50.0);
    }
}

#[test]
fn wraps_and_aligns_rows_and_supports_column_reverse() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            size(130.0, 100.0),
            flex::FlexWrap(taffy::FlexWrap::Wrap),
            flex::Gap(Size::new(StyleUnit::Px(10.0), StyleUnit::Px(5.0))),
            flex::AlignContent(taffy::AlignContent::START),
        ),
    );
    let children: Vec<_> = (0..3)
        .map(|_| {
            let node = ui.create(Node::Box, (size(60.0, 20.0),));
            ui.append(root, node);

            node
        })
        .collect();

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, children[1]).location.x, 70.0);
    close(box_for(&tree, key, children[2]).location.y, 25.0);

    ui.set_props(
        root,
        (
            flex::FlexDirection(taffy::FlexDirection::ColumnReverse),
            flex::FlexWrap(taffy::FlexWrap::NoWrap),
            flex::AlignItems(taffy::AlignItems::CENTER),
        ),
    );

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, children[0]).location.y, 80.0);
    close(box_for(&tree, key, children[1]).location.y, 55.0);
    close(box_for(&tree, key, children[2]).location.x, 35.0);
}

#[test]
fn alignment_and_item_override_use_fixed_cross_size() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            size(200.0, 100.0),
            flex::JustifyContent(taffy::JustifyContent::SPACE_BETWEEN),
            flex::AlignItems(taffy::AlignItems::CENTER),
        ),
    );
    let a = ui.create(Node::Box, (size(40.0, 20.0),));
    let b = ui.create(
        Node::Box,
        (
            size(40.0, 20.0),
            flex::AlignSelf(Some(taffy::AlignSelf::END)),
        ),
    );

    ui.append(root, a);
    ui.append(root, b);

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, a).location.y, 40.0);
    close(box_for(&tree, key, b).location.x, 160.0);
    close(box_for(&tree, key, b).location.y, 80.0);
}

#[test]
fn font_and_viewport_units_use_the_source_node_context() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            size(500.0, 100.0),
            bx::FontSize(StyleUnit::Px(20.0)),
            flex::Gap(Size::new(StyleUnit::Em(1.0), StyleUnit::ZERO)),
        ),
    );
    let a = ui.create(
        Node::Box,
        (
            bx::FontSize(StyleUnit::Px(30.0)),
            flex::FlexBasis(AutoValue::Value(StyleUnit::Em(2.0))),
        ),
    );
    let b = ui.create(
        Node::Box,
        (flex::FlexBasis(AutoValue::Value(StyleUnit::Rem(2.0))),),
    );
    let c = ui.create(
        Node::Box,
        (flex::FlexBasis(AutoValue::Value(StyleUnit::Vw(0.1))),),
    );
    let d = ui.create(
        Node::Box,
        (flex::FlexBasis(AutoValue::Value(StyleUnit::Vh(0.1))),),
    );

    for child in [a, b, c, d] {
        ui.append(root, child);
    }

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, a).size.width, 60.0);
    close(box_for(&tree, key, b).size.width, 32.0);
    close(box_for(&tree, key, b).location.x, 80.0);
    close(box_for(&tree, key, c).size.width, 80.0);
    close(box_for(&tree, key, d).size.width, 60.0);
}

#[test]
fn wrapped_text_and_inline_children_use_source_flex_properties() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            size(300.0, 100.0),
        ),
    );
    let text = ui.create(
        Node::Text,
        (
            Text("first item with wrapping words".into()),
            flex::FlexBasis(AutoValue::Value(StyleUnit::Percent(0.3))),
        ),
    );
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline, flex::FlexGrow(1.0)));
    let inside = ui.create(Node::Text, (Text("second item".into()),));

    ui.append(root, text);
    ui.append(root, span);
    ui.append(span, inside);

    let (tree, key) = layout(&ui, root);
    let children: Vec<_> = tree.boxes.children(key).collect();

    assert_eq!(children.len(), 2);
    close(tree.geometry(children[0]).size.width, 90.0);
    close(tree.geometry(children[1]).location.x, 90.0);
    close(tree.geometry(children[1]).size.width, 210.0);

    // Each child establishes the inline formatting context of its text.
    assert!(tree.has_glyphs_of(children[0], text));
    assert!(tree.has_glyphs_of(children[1], inside));
}

#[test]
fn nested_flex_and_incremental_style_changes_match_fresh_layout() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            size(300.0, 100.0),
        ),
    );
    let nested = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            flex::FlexGrow(1.0),
        ),
    );
    let a = ui.create(Node::Box, (flex::FlexGrow(1.0),));
    let b = ui.create(Node::Box, (flex::FlexGrow(1.0),));
    let sibling = ui.create(Node::Box, (size(50.0, 20.0),));

    ui.append(root, nested);
    ui.append(root, sibling);
    ui.append(nested, a);
    ui.append(nested, b);

    let mut retained = Layout::default();

    for step in 0..3 {
        if step == 1 {
            ui.set_props(
                nested,
                (flex::Gap(Size::new(StyleUnit::Px(10.0), StyleUnit::ZERO)),),
            );
            ui.set_props(b, (flex::FlexGrow(2.0),));
            let (nested, b) = (retained.item(nested), retained.item(b));
            retained
                .dirty
                .mark(&retained.inputs, nested, DirtyFlags::NEED_FULL_LAYOUT);
            retained
                .dirty
                .mark(&retained.inputs, b, DirtyFlags::NEED_FULL_LAYOUT);
        } else if step == 2 {
            ui.prepend(nested, b);
            let nested = retained.item(nested);
            retained
                .dirty
                .mark(&retained.inputs, nested, DirtyFlags::NEED_CHILDREN);
        }

        let current = retained.build(&ui, root);
        let (fresh, fresh_root) = layout(&ui, root);

        for node in [root, nested, a, b, sibling] {
            assert_eq!(
                box_for(&retained, current, node),
                box_for(&fresh, fresh_root, node)
            );
        }

        close(box_for(&retained, current, nested).size.width, 250.0);

        if step == 0 {
            close(box_for(&retained, current, a).size.width, 125.0);
        } else {
            close(box_for(&retained, current, a).size.width, 80.0);
            close(box_for(&retained, current, b).size.width, 160.0);
        }
    }
}

#[test]
fn absolute_children_and_descendants_stay_out_of_flex_sizing() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            size(200.0, 100.0),
        ),
    );
    let block = ui.create(Node::Box, (size(50.0, 40.0),));
    let absolute = ui.create(
        Node::Box,
        (
            size(20.0, 10.0),
            bx::Position(taffy::Position::Absolute),
            bx::Inset(Rect::new_invidiual(
                AutoValue::Value(StyleUnit::Px(7.0)),
                AutoValue::Auto,
                AutoValue::Auto,
                AutoValue::Value(StyleUnit::Px(11.0)),
            )),
        ),
    );
    let nested = ui.create(
        Node::Box,
        (
            size(5.0, 5.0),
            bx::Position(taffy::Position::Absolute),
            bx::Inset(Rect::new_all(AutoValue::Value(StyleUnit::Px(3.0)))),
        ),
    );

    ui.append(root, block);
    ui.append(root, absolute);
    ui.append(block, nested);

    let (tree, key) = layout(&ui, root);

    close(box_for(&tree, key, block).size.width, 50.0);
    close(box_for(&tree, key, absolute).location.x, 11.0);
    close(box_for(&tree, key, absolute).location.y, 7.0);
    close(box_for(&tree, key, nested).location.x, 3.0);
    close(box_for(&tree, key, nested).location.y, 3.0);
}

#[test]
fn text_items_align_by_their_first_baseline() {
    let mut ui = Ui::new();
    let root = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            size(300.0, 100.0),
            flex::AlignItems(taffy::AlignItems::BASELINE),
        ),
    );
    let a = ui.create(
        Node::Text,
        (Text("small".into()), bx::FontSize(StyleUnit::Px(12.0))),
    );
    let b = ui.create(
        Node::Text,
        (Text("large".into()), bx::FontSize(StyleUnit::Px(32.0))),
    );

    ui.append(root, a);
    ui.append(root, b);

    let (tree, key) = layout(&ui, root);

    close(tree.first_baseline(key, a), tree.first_baseline(key, b));
}

#[test]
fn viewport_gaps_follow_viewport_changes() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    // Fixed size, so the row's own layout input does not change.
    let wrapper = ui.create(Node::Box, (size(600.0, 50.0),));
    let row = ui.create(
        Node::Box,
        (
            bx::DisplayInner::Container(flex_layout()),
            flex::Gap(Size::new(StyleUnit::Vw(0.1), StyleUnit::ZERO)),
        ),
    );
    let a = ui.create(Node::Box, (size(50.0, 20.0),));
    let b = ui.create(Node::Box, (size(50.0, 20.0),));
    ui.append(root, wrapper);
    ui.append(wrapper, row);
    ui.append(row, a);
    ui.append(row, b);

    let mut layout = Layout::default();
    let second_x = |layout: &mut Layout, width: f64, viewport_changed: bool| {
        let viewport = Size::new(width, 600.0);
        let key = layout
            .input_builder
            .build(
                &ui,
                &mut layout.inputs,
                &mut layout.dirty,
                BuildRequest::new(root, 16.0, viewport),
            )
            .unwrap();
        let fragment = layout
            .builder
            .build(
                &ui,
                &layout.inputs,
                &ContentManager::new(),
                &mut layout.dirty,
                key,
                &mut layout.fragments,
                &LayoutConstraint::new_root(viewport, 16.0, 1.0)
                    .with_viewport_changed(viewport_changed),
            )
            .unwrap();
        layout.dirty.clear(&layout.inputs, key);

        box_for(layout, fragment, b).location.x
    };

    close(second_x(&mut layout, 400.0, false), 50.0 + 40.0);
    // The row reports the viewport unit of its gap, so it is laid out again.
    close(second_x(&mut layout, 800.0, true), 50.0 + 80.0);
}
