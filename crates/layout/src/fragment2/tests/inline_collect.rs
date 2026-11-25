use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

use crate::{
    dirty_map::DirtyMap,
    fragment2::{
        BoxFragmentType, InlineLayout, OffsetEntry,
        inline::collect::{FlowItem, InlineBuildContext, InlineContentBuilder, split_runs},
        inline_box_source,
    },
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{LayoutConstraint, bx, geometry::Size},
};

const VIEWPORT: Size<f64> = Size::new(400.0, 600.0);
const CONSTRAINT: LayoutConstraint = LayoutConstraint::new_root(VIEWPORT, 16.0);

struct Fixture {
    ui: Ui,
    tree: LayoutInputTree,
    root: LayoutInputKey,
    inline: InlineLayout,
    children: Vec<(LayoutInputKey, BoxFragmentType)>,
}

impl Fixture {
    /// Collect and shape the inline content of `root`.
    fn new(ui: Ui, root: NodeKey) -> Self {
        let mut tree = LayoutInputTree::new();
        let root = LayoutInputTreeBuilder::new()
            .build(
                &ui,
                &mut tree,
                &mut DirtyMap::new(),
                BuildRequest::new(root, 16.0, VIEWPORT),
            )
            .unwrap();

        let mut fixture = Self {
            ui,
            tree,
            root,
            inline: InlineLayout::new(),
            children: Vec::new(),
        };
        fixture.build_run(None);

        fixture
    }

    fn cx(&self) -> InlineBuildContext<'_> {
        InlineBuildContext {
            ui: &self.ui,
            tree: &self.tree,
            constraint: &CONSTRAINT,
        }
    }

    /// Collect and shape the run of the inline content after `start`.
    fn build_run(&mut self, start: Option<LayoutInputKey>) {
        let mut inline = std::mem::take(&mut self.inline);
        let mut builder = InlineContentBuilder::new();
        builder.build(
            &self.cx(),
            &mut parley::FontContext::new(),
            self.root,
            start,
            &mut inline,
        );
        self.inline = inline;
        self.children = builder.children.clone();
    }

    fn item(&self, node: NodeKey) -> LayoutInputKey {
        self.tree.item_for_node(node).unwrap()
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

    let f = Fixture::new(ui, root);
    let inline = &f.inline;

    // White space is kept as is.
    assert_eq!(inline.text, "a  b c\u{FFFC}\u{FFFC}d");
    assert!(inline.items.is_empty(), "not laid out");
    assert!(
        inline.atomics.is_empty(),
        "left to the fragment tree builder"
    );
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
    assert_eq!(f.children, [(f.item(badge), BoxFragmentType::AtomicInline)]);
}

#[test]
fn block_boxes_split_the_content_into_runs() {
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

    let mut f = Fixture::new(ui, root);
    let [first, second, last] = [first, second, last].map(|node| f.item(node));

    // Adjacent blocks have no run between them, and none follows the last.
    let mut flow = Vec::new();
    split_runs(&f.cx(), f.root, &mut flow);
    assert_eq!(
        flow,
        [
            FlowItem::Run(None),
            FlowItem::Block(first),
            FlowItem::Block(second),
            FlowItem::Run(Some(second)),
            FlowItem::Block(last),
        ]
    );

    assert_eq!(f.inline.text, "a", "the first run ends at the block");
    assert!(f.children.is_empty());

    f.build_run(Some(second));
    assert_eq!(f.inline.text, "b");
    assert_eq!(f.inline.offset_mapping[0].source, f.item(after));
}

#[test]
fn inline_elements_split_by_blocks_continue_across_runs() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let before = text(&mut ui, "ab");
    let block = ui.create(Node::Box, ());
    let after = text(&mut ui, "cd");
    let tail = text(&mut ui, "e");
    ui.append(root, span);
    ui.append(span, before);
    ui.append(span, block);
    ui.append(span, after);
    ui.append(root, tail);

    let mut f = Fixture::new(ui, root);
    let (span, block) = (f.item(span), f.item(block));

    assert_eq!(f.inline.text, "ab");
    assert!(f.inline.reopened.is_empty());
    assert_eq!(f.inline.continued, [span]);
    assert_eq!(f.inline.range(span), Some(0..2));

    f.build_run(Some(block));
    assert_eq!(f.inline.text, "cde");
    assert_eq!(f.inline.reopened, [span]);
    assert!(f.inline.continued.is_empty());
    assert_eq!(f.inline.range(span), Some(0..2));
    assert_eq!(
        f.inline.layout.inline_boxes().count(),
        0,
        "the rest of a split element is no empty element"
    );
}

#[test]
fn split_elements_without_content_make_no_runs() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let block = ui.create(Node::Box, ());
    let empty = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    ui.append(root, span);
    ui.append(span, block);
    ui.append(root, empty);

    let f = Fixture::new(ui, root);
    let mut flow = Vec::new();
    split_runs(&f.cx(), f.root, &mut flow);

    // The span has no content before or after the block, but the empty
    // element after it is content.
    assert_eq!(
        flow,
        [
            FlowItem::Block(f.item(block)),
            FlowItem::Run(Some(f.item(block))),
        ]
    );
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

    let f = Fixture::new(ui, root);
    let inline = &f.inline;

    assert_eq!(inline.text, "abc");
    let inline_box = inline.layout.inline_boxes().next().unwrap();
    assert_eq!(inline_box.kind, parley::InlineBoxKind::OutOfFlow);
    assert_eq!(inline_box.index, 2);
    assert_eq!(f.children, [(f.item(absolute), BoxFragmentType::OutOfFlow)]);
}

#[test]
fn glyph_runs_are_branded_with_their_items() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let first = text(&mut ui, "first");
    let second = text(&mut ui, "second");
    ui.append(root, first);
    ui.append(root, second);

    let mut f = Fixture::new(ui, root);
    f.inline.layout.break_all_lines(None);
    let brushes: Vec<_> = f
        .inline
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

    let f = Fixture::new(ui, root);

    assert_eq!(f.inline.text, "alone");
    assert_eq!(f.inline.offset_mapping[0].source, f.root);
}

#[test]
fn building_again_reuses_the_layout() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let leaf = text(&mut ui, "some text");
    ui.append(root, leaf);

    let mut f = Fixture::new(ui, root);
    let text_ptr = f.inline.text.as_ptr();

    let mut builder = InlineContentBuilder::new();
    builder.build(
        &InlineBuildContext {
            ui: &f.ui,
            tree: &f.tree,
            constraint: &CONSTRAINT,
        },
        &mut parley::FontContext::new(),
        f.root,
        None,
        &mut f.inline,
    );

    assert_eq!(f.inline.text, "some text");
    assert_eq!(
        f.inline.text.as_ptr(),
        text_ptr,
        "the text allocation is kept"
    );
}
