use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

use crate::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::state::NodeLayoutBits,
    fragment2::{
        BoxFragment, BoxFragmentKey, BoxFragmentType, ChildLayoutType, FragmentTree, InlineLayout,
        OffsetEntry,
        inline::builder::{InlineBuildContext, InlineLayoutBuilder},
        inline_box_source,
    },
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

const VIEWPORT: Size<f64> = Size::new(400.0, 600.0);
const CONSTRAINT: LayoutConstraint = LayoutConstraint::new_root(VIEWPORT, 16.0);

/// Built boxes inside inline content, in the order they were yielded.
type Built = Vec<(LayoutInputKey, BoxFragmentType)>;

/// Build `root` and drain its fragmentainer. Boxes inside the inline content
/// are leaves, or with `nested`, inline formatting contexts built with the
/// same builder.
#[allow(clippy::too_many_arguments)]
fn build_tree(
    builder: &mut InlineLayoutBuilder,
    cx: &mut InlineBuildContext<'_>,
    fragments: &mut FragmentTree,
    root: LayoutInputKey,
    box_type: BoxFragmentType,
    old: Option<BoxFragmentKey>,
    nested: bool,
    built: &mut Built,
) -> BoxFragmentKey {
    let (key, mut children) = builder.build(cx, fragments, root, box_type, old);

    while let Some(child) = children.next(fragments) {
        assert_eq!(child.parent, key);
        built.push((child.item, child.box_type));

        let child_key = if nested {
            build_tree(
                builder,
                cx,
                fragments,
                child.item,
                child.box_type,
                child.old,
                nested,
                built,
            )
        } else if let Some(old) = child.old {
            old
        } else {
            let (style, _) = cx.resolve_style(child.item);
            fragments.insert(BoxFragment::new(
                child.item,
                child.box_type,
                ChildLayoutType::Content,
                style,
            ))
        };

        child.attach(fragments, child_key);
    }

    key
}

struct Fixture {
    ui: Ui,
    tree: LayoutInputTree,
    fonts: parley::FontContext,
    builder: InlineLayoutBuilder,
    fragments: FragmentTree,
}

impl Fixture {
    fn new(ui: Ui) -> Self {
        Self {
            ui,
            tree: LayoutInputTree::new(),
            fonts: parley::FontContext::new(),
            builder: InlineLayoutBuilder::new(),
            fragments: FragmentTree::new(),
        }
    }

    fn input(&mut self, root: NodeKey) -> LayoutInputKey {
        LayoutInputTreeBuilder::new()
            .build(
                &self.ui,
                &mut self.tree,
                &mut DirtyMap::new(),
                BuildRequest::new(root, 16.0, VIEWPORT),
            )
            .unwrap()
    }

    fn build(
        &mut self,
        root: LayoutInputKey,
        old: Option<BoxFragmentKey>,
        nested: bool,
    ) -> (BoxFragmentKey, Built) {
        let (mut cx, builder, fragments) = self.context();

        let mut built = Built::new();
        let key = build_tree(
            builder,
            &mut cx,
            fragments,
            root,
            BoxFragmentType::Normal,
            old,
            nested,
            &mut built,
        );

        (key, built)
    }

    fn context(
        &mut self,
    ) -> (
        InlineBuildContext<'_>,
        &mut InlineLayoutBuilder,
        &mut FragmentTree,
    ) {
        (
            InlineBuildContext {
                ui: &self.ui,
                tree: &self.tree,
                fonts: &mut self.fonts,
                constraint: &CONSTRAINT,
            },
            &mut self.builder,
            &mut self.fragments,
        )
    }

    /// Build the input tree again after the children of `node` changed.
    fn rebuild_input(&mut self, root: NodeKey, node: NodeKey) {
        let mut dirty = DirtyMap::new();
        dirty.mark_node(&self.tree, node, DirtyFlags::NEED_CHILDREN);
        LayoutInputTreeBuilder::new()
            .build(
                &self.ui,
                &mut self.tree,
                &mut dirty,
                BuildRequest::new(root, 16.0, VIEWPORT),
            )
            .unwrap();
    }

    fn item(&self, node: NodeKey) -> LayoutInputKey {
        self.tree.item_for_node(node).unwrap()
    }

    fn inline(&self, key: BoxFragmentKey) -> &InlineLayout {
        self.fragments.inline_layout(key).unwrap()
    }
}

fn text(ui: &mut Ui, value: &str) -> NodeKey {
    ui.create(Node::Text, (Text(value.into()),))
}

fn inline_block(ui: &mut Ui) -> NodeKey {
    ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    )
}

#[test]
fn concatenates_text_with_boxes_and_offsets() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "a  b ");
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let inner = text(&mut ui, "c");
    let empty = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let badge = inline_block(&mut ui);
    let after = text(&mut ui, "d");
    ui.append(root, before);
    ui.append(root, span);
    ui.append(span, inner);
    ui.append(span, empty);
    ui.append(span, badge);
    ui.append(root, after);

    let mut f = Fixture::new(ui);
    let root = f.input(root);
    let (key, leaves) = f.build(root, None, false);
    let inline = f.inline(key);

    // White space is kept as is.
    assert_eq!(inline.text, "a  b c\u{FFFC}\u{FFFC}d");
    assert!(inline.items.is_empty(), "not laid out");
    assert_eq!(
        inline.offset_mapping,
        [
            OffsetEntry {
                source: f.item(before),
                range: 0..5,
                text: true
            },
            OffsetEntry {
                source: f.item(span),
                range: 5..12,
                text: false
            },
            OffsetEntry {
                source: f.item(inner),
                range: 5..6,
                text: true
            },
            OffsetEntry {
                source: f.item(empty),
                range: 6..9,
                text: false
            },
            OffsetEntry {
                source: f.item(after),
                range: 12..13,
                text: true
            },
        ]
    );

    let boxes: Vec<_> = inline
        .layout
        .inline_boxes()
        .map(|inline_box| (inline_box_source(inline_box.id), inline_box.index))
        .collect();
    assert_eq!(boxes, [(f.item(empty), 6), (f.item(badge), 9)]);

    assert_eq!(leaves, [(f.item(badge), BoxFragmentType::AtomicInline)]);
    let children: Vec<_> = f.fragments.boxes.children(key).collect();
    assert_eq!(children, [f.fragments.box_fragment(f.item(badge)).unwrap()]);
    assert_eq!(f.fragments.box_fragment(root), Some(key));
    assert!(f.fragments.boxes.parent(key).is_none(), "detached");
}

#[test]
fn block_boxes_take_lines_of_their_own() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "a");
    let first = ui.create(Node::Box, ());
    let second = ui.create(Node::Box, ());
    let after = text(&mut ui, "b");
    let last = ui.create(Node::Box, ());
    ui.append(root, before);
    ui.append(root, first);
    ui.append(root, second);
    ui.append(root, after);
    ui.append(root, last);

    let mut f = Fixture::new(ui);
    let root = f.input(root);
    let (key, leaves) = f.build(root, None, false);

    // Adjacent blocks share no empty line, and the last leaves no
    // trailing one.
    assert_eq!(f.inline(key).text, "a\n\u{FFFC}\n\u{FFFC}\nb\n\u{FFFC}");
    assert!(
        leaves
            .iter()
            .all(|&(_, box_type)| box_type == BoxFragmentType::InlinePart)
    );
    assert_eq!(leaves.len(), 3);
}

#[test]
fn absolute_boxes_are_out_of_flow_without_text() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "ab");
    let absolute = ui.create(Node::Box, (bx::Position(taffy::Position::Absolute),));
    let after = text(&mut ui, "c");
    ui.append(root, before);
    ui.append(root, absolute);
    ui.append(root, after);

    let mut f = Fixture::new(ui);
    let root = f.input(root);
    let (key, leaves) = f.build(root, None, false);
    let inline = f.inline(key);

    assert_eq!(inline.text, "abc");
    let inline_box = inline.layout.inline_boxes().next().unwrap();
    assert_eq!(inline_box.kind, parley::InlineBoxKind::OutOfFlow);
    assert_eq!(inline_box.index, 2);
    assert_eq!(leaves, [(f.item(absolute), BoxFragmentType::OutOfFlow)]);
}

#[test]
fn glyph_runs_are_branded_with_their_items() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let first = text(&mut ui, "first");
    let second = text(&mut ui, "second");
    ui.append(root, first);
    ui.append(root, second);

    let mut f = Fixture::new(ui);
    let root = f.input(root);
    let key = f.build(root, None, false).0;
    let inline = f.fragments.get_mut(key).unwrap().inline_mut().unwrap();

    inline.layout.break_all_lines(None);
    let brushes: Vec<_> = inline
        .layout
        .lines()
        .flat_map(|line| line.items().collect::<Vec<_>>())
        .filter_map(|item| match item {
            parley::PositionedLayoutItem::GlyphRun(run) => Some(run.style().brush),
            _ => None,
        })
        .collect();

    assert_eq!(brushes, [f.item(first), f.item(second)]);
}

#[test]
fn text_root_contains_itself() {
    let mut ui = Ui::new();
    let root = text(&mut ui, "alone");

    let mut f = Fixture::new(ui);
    let root = f.input(root);
    let key = f.build(root, None, false).0;

    assert_eq!(f.inline(key).text, "alone");
}

#[test]
fn reuses_inline_layouts_of_the_old_subtree() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let badge = inline_block(&mut ui);
    let inner = text(&mut ui, "inner");
    let outer = text(&mut ui, "outer");
    ui.append(root, badge);
    ui.append(badge, inner);
    ui.append(root, outer);

    let mut f = Fixture::new(ui);
    let root = f.input(root);
    let old = f.build(root, None, true).0;
    let old_child = f.fragments.box_fragment(f.item(badge)).unwrap();
    let mut before = [
        f.inline(old).text.as_ptr(),
        f.inline(old_child).text.as_ptr(),
    ];
    before.sort();

    let key = f.build(root, Some(old), true).0;
    let child = f.fragments.box_fragment(f.item(badge)).unwrap();

    assert!(f.fragments.get(old).is_none(), "old root deleted");
    assert!(f.fragments.get(old_child).is_none(), "old subtree deleted");
    assert_eq!(f.fragments.boxes.len(), 2);
    assert_eq!(f.inline(key).text, "\u{FFFC}outer");
    assert_eq!(f.inline(child).text, "inner");

    // Equal lengths fit the old allocations, wherever each layout went.
    let mut after = [f.inline(key).text.as_ptr(), f.inline(child).text.as_ptr()];
    after.sort();
    assert_eq!(after, before, "text allocations kept");
}

#[test]
fn ignores_old_boxes_of_other_items() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let other = ui.create(Node::Box, ());
    let content = text(&mut ui, "content");
    let other_content = text(&mut ui, "other");
    ui.append(root, content);
    ui.append(other, other_content);

    let mut f = Fixture::new(ui);
    let other = f.input(other);
    let root = f.input(root);
    let unrelated = f.build(other, None, false).0;

    let key = f.build(root, Some(unrelated), false).0;

    assert!(f.fragments.get(unrelated).is_some(), "kept");
    assert_eq!(f.inline(key).text, "content");
}

#[test]
fn builds_nested_inline_formatting_contexts() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let badge = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            bx::DisplayInner::FlowRoot,
            bx::Size(Size::new(
                AutoValue::Value(StyleUnit::Vw(0.1)),
                AutoValue::Auto,
            )),
        ),
    );
    let inner = text(&mut ui, "inner");
    let outer = text(&mut ui, "outer");
    ui.append(root, badge);
    ui.append(badge, inner);
    ui.append(root, outer);

    let mut f = Fixture::new(ui);
    let root = f.input(root);
    let key = f.build(root, None, true).0;

    let child = f.fragments.box_fragment(f.item(badge)).unwrap();
    assert_eq!(f.fragments.boxes.parent(child), Some(key));
    assert_eq!(
        f.fragments.get(child).unwrap().box_type,
        BoxFragmentType::AtomicInline
    );
    assert_eq!(f.inline(child).text, "inner");
    assert_eq!(f.inline(key).text, "\u{FFFC}outer");
    assert!(
        f.fragments
            .get(child)
            .unwrap()
            .bits()
            .contains(NodeLayoutBits::DEPENDS_VIEWPORT),
        "style bits resolved"
    );
}

#[test]
fn boxes_attach_after_the_previous_box_even_if_anonymous() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let first = inline_block(&mut ui);
    let middle = text(&mut ui, "middle");
    let second = inline_block(&mut ui);
    ui.append(root, first);
    ui.append(root, middle);
    ui.append(root, second);

    let mut f = Fixture::new(ui);
    let root = f.input(root);
    let (mut cx, builder, fragments) = f.context();
    let (key, mut children) =
        builder.build(&mut cx, fragments, root, BoxFragmentType::Normal, None);
    assert_eq!(children.len(), 2);

    // Each box is wrapped in an anonymous box, attached in its place.
    let mut wrappers = Vec::new();
    while let Some(child) = children.next(fragments) {
        assert_eq!(child.after, wrappers.last().copied());

        let (style, _) = cx.resolve_style(child.item);
        let built = fragments.insert(BoxFragment::new(
            child.item,
            child.box_type,
            ChildLayoutType::Content,
            style.clone(),
        ));
        let wrapper = fragments.insert_anonymous(BoxFragment::new(
            key_source(fragments, key),
            BoxFragmentType::Normal,
            ChildLayoutType::Block,
            style,
        ));
        fragments.boxes.append(wrapper, built);
        child.attach(fragments, wrapper);
        wrappers.push(wrapper);
    }

    assert!(children.is_empty());
    assert_eq!(fragments.boxes.children(key).collect::<Vec<_>>(), wrappers);
    assert!(
        wrappers
            .iter()
            .all(|&wrapper| fragments.is_anonymous(wrapper))
    );
}

fn key_source(fragments: &FragmentTree, key: BoxFragmentKey) -> LayoutInputKey {
    fragments.get(key).unwrap().source
}

#[test]
fn reuses_atomic_boxes_of_the_old_root() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let first = inline_block(&mut ui);
    let middle = text(&mut ui, "middle");
    let second = inline_block(&mut ui);
    ui.append(root, first);
    ui.append(root, middle);
    ui.append(root, second);

    let mut f = Fixture::new(ui);
    let root = f.input(root);
    let old = f.build(root, None, false).0;
    let boxes = [
        f.fragments.box_fragment(f.item(first)).unwrap(),
        f.fragments.box_fragment(f.item(second)).unwrap(),
    ];
    assert_eq!(f.inline(old).atomics.len(), 2);

    // The leaves attach the old boxes again.
    let key = f.build(root, Some(old), false).0;

    assert!(f.fragments.get(old).is_none(), "old root deleted");
    assert_eq!(f.fragments.boxes.children(key).collect::<Vec<_>>(), boxes);
    let atomics = &f.inline(key).atomics;
    assert_eq!(atomics[&f.item(first)], boxes[0]);
    assert_eq!(atomics[&f.item(second)], boxes[1]);
    assert_eq!(f.fragments.boxes.len(), 3);
}

#[test]
fn yields_old_boxes_and_deletes_those_not_reused() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let first = inline_block(&mut ui);
    let second = inline_block(&mut ui);
    ui.append(root, first);
    ui.append(root, second);

    let mut f = Fixture::new(ui);
    let root = f.input(root);
    let old = f.build(root, None, false).0;
    let old_boxes = [
        f.fragments.box_fragment(f.item(first)).unwrap(),
        f.fragments.box_fragment(f.item(second)).unwrap(),
    ];

    let (mut cx, builder, fragments) = f.context();
    let (key, mut children) =
        builder.build(&mut cx, fragments, root, BoxFragmentType::Normal, Some(old));

    // The first old box is reused, the second replaced by a new box.
    let child = children.next(fragments).unwrap();
    assert_eq!(child.old, Some(old_boxes[0]));
    assert!(fragments.boxes.parent(old_boxes[0]).is_none(), "detached");
    child.attach(fragments, old_boxes[0]);

    let child = children.next(fragments).unwrap();
    assert_eq!(child.old, Some(old_boxes[1]));
    let (style, _) = cx.resolve_style(child.item);
    let replaced = fragments.insert(BoxFragment::new(
        child.item,
        child.box_type,
        ChildLayoutType::Content,
        style,
    ));
    child.attach(fragments, replaced);

    assert!(children.next(fragments).is_none());
    assert!(
        fragments.get(old_boxes[1]).is_none(),
        "unused old box deleted"
    );
    assert_eq!(
        fragments.boxes.children(key).collect::<Vec<_>>(),
        [old_boxes[0], replaced]
    );

    let atomics = &fragments.inline_layout(key).unwrap().atomics;
    assert_eq!(atomics.get(&child.item), Some(&replaced));
    assert_eq!(atomics.len(), 2);
}

#[test]
fn deletes_old_boxes_of_removed_items() {
    let mut ui = Ui::new();
    let root_node = ui.create(Node::Box, ());
    let kept = inline_block(&mut ui);
    let removed = inline_block(&mut ui);
    ui.append(root_node, kept);
    ui.append(root_node, removed);

    let mut f = Fixture::new(ui);
    let root = f.input(root_node);
    let old = f.build(root, None, false).0;
    let removed_box = f.fragments.box_fragment(f.item(removed)).unwrap();

    f.ui.delete(removed);
    f.rebuild_input(root_node, root_node);
    let key = f.build(root, Some(old), false).0;

    assert!(f.fragments.get(removed_box).is_none(), "deleted");
    assert_eq!(f.inline(key).atomics.len(), 1);
    assert_eq!(f.fragments.boxes.len(), 2);
}
