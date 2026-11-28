use comet_content::manager::ContentManager;
use std::num::NonZeroU64;

use comet_content::ContentId;
use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

use crate::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{
        BoxFragmentKey, BoxFragmentType, NodeLayoutBits, builder::FragmentTreeBuilder,
        tree::FragmentTree,
    },
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{LayoutConstraint, bx},
};
use comet_geometry::{AutoValue, Point, Rect, Size, StyleUnit};

pub(super) const VIEWPORT: Size<f64> = Size::new(400.0, 600.0);
const CONSTRAINT: LayoutConstraint = LayoutConstraint::new_root(VIEWPORT, 16.0, 1.0);

pub(super) struct Fixture {
    pub(super) ui: Ui,
    pub(super) root: NodeKey,
    pub(super) tree: LayoutInputTree,
    pub(super) dirty: DirtyMap,
    pub(super) input: LayoutInputTreeBuilder,
    pub(super) builder: FragmentTreeBuilder,
    fonts: parley::FontContext,
    pub(super) fragments: FragmentTree,
    pub(super) layout_root: Option<LayoutInputKey>,
    pub(super) root_box: Option<BoxFragmentKey>,
    pub(super) constraint: LayoutConstraint,
}

impl Fixture {
    pub(super) fn new(ui: Ui, root: NodeKey) -> Self {
        let mut fixture = Self {
            ui,
            root,
            tree: LayoutInputTree::new(),
            dirty: DirtyMap::new(),
            input: LayoutInputTreeBuilder::new(),
            builder: FragmentTreeBuilder::new(),
            fonts: parley::FontContext::new(),
            fragments: FragmentTree::new(),
            layout_root: None,
            root_box: None,
            constraint: CONSTRAINT,
        };
        fixture.layout();

        fixture
    }

    /// Build the input tree, and update the fragment tree with the marked
    /// changes without laying it out.
    pub(super) fn build(&mut self) -> BoxFragmentKey {
        self.run(false)
    }

    /// Build the input tree and the fragment tree, laying it out.
    pub(super) fn layout(&mut self) -> BoxFragmentKey {
        self.run(true)
    }

    fn run(&mut self, layout: bool) -> BoxFragmentKey {
        let layout_root = self
            .input
            .build(
                &self.ui,
                &mut self.tree,
                &mut self.dirty,
                BuildRequest::new(self.root, 16.0, VIEWPORT),
            )
            .unwrap();
        let key = if layout {
            self.builder.build(
                &self.ui,
                &self.tree,
                &ContentManager::new(),
                &mut self.fonts,
                &mut self.dirty,
                layout_root,
                &mut self.fragments,
                &self.constraint,
            )
        } else {
            self.builder.update(
                &self.ui,
                &self.tree,
                &ContentManager::new(),
                &mut self.fonts,
                &mut self.dirty,
                layout_root,
                &mut self.fragments,
                &self.constraint,
            )
        }
        .unwrap();

        self.layout_root = Some(layout_root);
        self.root_box = Some(key);
        self.check(key);

        key
    }

    /// Start a new round of changes: clear the flags, and mark every box as
    /// laid out.
    pub(super) fn next_round(&mut self) {
        self.dirty.clear(&self.tree, self.layout_root.unwrap());

        let keys: Vec<_> = self
            .fragments
            .boxes
            .descendants(self.root_box.unwrap())
            .collect();
        for key in keys {
            let fragment = &mut self.fragments.boxes[key];
            fragment.final_layout_valid = true;
        }
    }

    pub(super) fn mark(&mut self, node: NodeKey, flags: DirtyFlags) {
        self.dirty.mark_node(&self.tree, node, flags);
    }

    pub(super) fn item(&self, node: NodeKey) -> LayoutInputKey {
        self.tree.item_for_node(node).unwrap()
    }

    pub(super) fn box_of(&self, node: NodeKey) -> BoxFragmentKey {
        self.fragments.box_fragment(self.item(node)).unwrap()
    }

    pub(super) fn has_box(&self, node: NodeKey) -> bool {
        self.tree
            .item_for_node(node)
            .and_then(|item| self.fragments.box_fragment(item))
            .is_some()
    }

    pub(super) fn children(&self, key: BoxFragmentKey) -> Vec<BoxFragmentKey> {
        self.fragments.boxes.children(key).collect()
    }

    pub(super) fn valid(&self, key: BoxFragmentKey) -> bool {
        self.fragments.boxes[key].final_layout_valid
    }

    /// The boxes of the root are all the boxes of the tree: nothing is left
    /// detached. The builder checks the rest itself once laid out.
    fn check(&self, root: BoxFragmentKey) {
        assert_eq!(
            self.fragments.boxes.descendants(root).count(),
            self.fragments.boxes.len()
        );
    }

    pub(super) fn inline_text(&self, node: NodeKey) -> &str {
        &self.fragments.boxes[self.box_of(node)]
            .inline()
            .unwrap()
            .text
    }

    /// The texts of the runs of a mixed inline box.
    pub(super) fn run_texts(&self, node: NodeKey) -> Vec<String> {
        self.fragments
            .runs(self.box_of(node))
            .map(|run| self.fragments.inline_layout(run).unwrap().text.clone())
            .collect()
    }

    pub(super) fn built(&self, node: NodeKey) -> bool {
        self.fragments.boxes[self.box_of(node)]
            .inline()
            .is_some_and(|inline| inline.is_built())
    }
}

pub(super) fn text(ui: &mut Ui, value: &str) -> NodeKey {
    ui.create(Node::Text, (Text(value.into()),))
}

/// A block containing a text.
pub(super) fn paragraph(ui: &mut Ui, value: &str) -> NodeKey {
    let block = ui.create(Node::Box, ());
    let text = text(ui, value);
    ui.append(block, text);

    block
}

#[test]
fn fresh_build_classifies_boxes() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = paragraph(&mut ui, "text");
    let section = ui.create(Node::Box, ());
    let empty = ui.create(Node::Box, ());
    let flow_root = ui.create(Node::Box, (bx::DisplayInner::FlowRoot,));
    let absolute = ui.create(Node::Box, (bx::Position(taffy::Position::Absolute),));
    let content = ui.create(
        Node::Box,
        (bx::DisplayInner::Content(ContentId::from_ffi(
            NonZeroU64::MIN,
        )),),
    );

    ui.append(root, para);
    ui.append(root, section);
    ui.append(section, empty);
    ui.append(root, flow_root);
    ui.append(root, absolute);
    ui.append(root, content);

    let fx = Fixture::new(ui, root);
    let get = |node| &fx.fragments.boxes[fx.box_of(node)];

    let root_box = get(root);
    assert_eq!(root_box.box_type, BoxFragmentType::Normal);
    assert!(root_box.child_layout.is_block());
    assert_eq!(
        fx.children(fx.box_of(root)),
        [para, section, flow_root, absolute, content].map(|node| fx.box_of(node))
    );

    assert!(get(para).child_layout.is_inline());
    assert_eq!(get(para).inline().unwrap().text, "text");
    assert!(get(section).child_layout.is_block());
    assert!(get(empty).child_layout.is_block());
    assert_eq!(get(flow_root).box_type, BoxFragmentType::FlowRoot);
    assert_eq!(get(absolute).box_type, BoxFragmentType::OutOfFlow);
    assert_eq!(get(content).box_type, BoxFragmentType::FlowRoot);
    assert!(get(content).child_layout.is_content());
}

#[test]
fn block_children_keep_their_keys() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = paragraph(&mut ui, "a");
    let b = paragraph(&mut ui, "b");
    let c = paragraph(&mut ui, "c");
    ui.append(root, a);
    ui.append(root, b);
    ui.append(root, c);

    let mut fx = Fixture::new(ui, root);
    let (root_box, a_box, b_box) = (fx.box_of(root), fx.box_of(a), fx.box_of(b));

    fx.next_round();
    let n = paragraph(&mut fx.ui, "n");
    fx.ui.before(b, n);
    fx.ui.after(b, a);
    fx.mark(root, DirtyFlags::NEED_CHILDREN);
    let c_item = fx.item(c);
    fx.ui.delete(c);

    assert_eq!(fx.build(), root_box);
    assert_eq!(fx.box_of(a), a_box);
    assert_eq!(fx.box_of(b), b_box);
    assert_eq!(
        fx.children(root_box),
        [fx.box_of(n), b_box, a_box],
        "boxes follow the new child order"
    );
    assert!(fx.fragments.box_fragment(c_item).is_none());

    assert!(!fx.valid(root_box));
    assert!(fx.valid(a_box) && fx.valid(b_box));
}

#[test]
fn text_change_builds_its_inline_formatting_context_again() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = ui.create(Node::Box, ());
    let before = text(&mut ui, "hello ");
    let badge = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    let badge_text = text(&mut ui, "badge");
    let other = paragraph(&mut ui, "other");

    ui.append(root, para);
    ui.append(para, before);
    ui.append(para, badge);
    ui.append(badge, badge_text);
    ui.append(root, other);

    let mut fx = Fixture::new(ui, root);
    let (root_box, para_box, badge_box, other_box) = (
        fx.box_of(root),
        fx.box_of(para),
        fx.box_of(badge),
        fx.box_of(other),
    );
    assert_eq!(
        fx.fragments.boxes[badge_box].box_type,
        BoxFragmentType::AtomicInline
    );
    assert_eq!(fx.children(para_box), [badge_box]);

    fx.next_round();
    fx.ui.set_props(before, (Text("goodbye ".into()),));
    fx.mark(before, DirtyFlags::NEED_FULL_LAYOUT);
    fx.build();

    assert_eq!(fx.box_of(para), para_box, "the box is kept");
    assert!(!fx.built(para), "built again when laid out");
    assert!(!fx.valid(para_box) && !fx.valid(root_box));
    assert!(fx.valid(badge_box), "boxes inside keep their layout");
    assert!(fx.valid(other_box));

    fx.layout();
    assert_eq!(fx.inline_text(para), "goodbye \u{FFFC}");
    assert_eq!(fx.children(root_box), [para_box, other_box]);
    assert_eq!(fx.box_of(badge), badge_box, "boxes inside are kept");
    assert_eq!(fx.children(para_box), [badge_box]);
}

#[test]
fn atomic_content_change_keeps_its_owner() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "hello ");
    let badge = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    let badge_text = text(&mut ui, "badge");

    ui.append(root, before);
    ui.append(root, badge);
    ui.append(badge, badge_text);

    let mut fx = Fixture::new(ui, root);
    let (root_box, badge_box) = (fx.box_of(root), fx.box_of(badge));

    fx.next_round();
    fx.ui.set_props(badge_text, (Text("changed".into()),));
    fx.mark(badge_text, DirtyFlags::NEED_FULL_LAYOUT);

    assert_eq!(fx.build(), root_box);
    assert!(fx.built(root), "the owner is not built again");
    assert!(!fx.built(badge));
    assert!(!fx.valid(root_box));

    fx.layout();
    assert_eq!(fx.box_of(badge), badge_box);
    assert_eq!(fx.inline_text(badge), "changed");
    assert_eq!(
        fx.fragments.boxes[badge_box].box_type,
        BoxFragmentType::AtomicInline
    );
    assert_eq!(fx.children(root_box), [badge_box]);
}

#[test]
fn root_inline_build_keeps_the_root_box() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let leaf = text(&mut ui, "one");
    ui.append(root, leaf);

    let mut fx = Fixture::new(ui, root);
    let root_box = fx.box_of(root);

    fx.next_round();
    fx.ui.set_props(leaf, (Text("two".into()),));
    fx.mark(leaf, DirtyFlags::NEED_FULL_LAYOUT);

    assert_eq!(fx.layout(), root_box);
    assert_eq!(fx.inline_text(root), "two");
}

#[test]
fn text_root_gets_an_anonymous_box() {
    let mut ui = Ui::new();
    let root = text(&mut ui, "one");

    let fx = Fixture::new(ui, root);
    let fragment = &fx.fragments.boxes[fx.box_of(root)];

    assert_eq!(fragment.box_type, BoxFragmentType::Anonymous);
    assert_eq!(fx.inline_text(root), "one");
}

#[test]
fn child_layout_kind_change_keeps_the_box() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let section = ui.create(Node::Box, ());
    let inner = paragraph(&mut ui, "inner");
    ui.append(root, section);
    ui.append(section, inner);

    let mut fx = Fixture::new(ui, root);
    let (section_box, inner_box) = (fx.box_of(section), fx.box_of(inner));
    assert!(fx.fragments.boxes[section_box].child_layout.is_block());

    fx.next_round();
    let leading = text(&mut fx.ui, "leading");
    fx.ui.prepend(section, leading);
    fx.mark(section, DirtyFlags::NEED_CHILDREN);
    fx.layout();

    assert_eq!(fx.box_of(section), section_box);
    assert!(
        fx.fragments.boxes[section_box]
            .child_layout
            .is_mixed_inline()
    );
    assert_eq!(fx.run_texts(section), ["leading"]);
    assert_eq!(fx.box_of(inner), inner_box, "child boxes are kept");
    assert_eq!(
        fx.fragments.boxes[inner_box].box_type,
        BoxFragmentType::Normal
    );

    // And back to block layout.
    fx.next_round();
    fx.ui.delete(leading);
    fx.mark(section, DirtyFlags::NEED_CHILDREN);
    fx.layout();

    assert_eq!(fx.box_of(section), section_box);
    assert!(fx.fragments.boxes[section_box].child_layout.is_block());
    assert_eq!(fx.box_of(inner), inner_box);
    assert_eq!(
        fx.fragments.boxes[inner_box].box_type,
        BoxFragmentType::Normal
    );
}

#[test]
fn flow_change_inside_inline_content_builds_the_owner_again() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = ui.create(Node::Box, ());
    let before = text(&mut ui, "hello ");
    let badge = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    ui.append(root, para);
    ui.append(para, before);
    ui.append(para, badge);

    let mut fx = Fixture::new(ui, root);
    let (para_box, badge_box) = (fx.box_of(para), fx.box_of(badge));

    // Staying in flow leaves the owner built.
    fx.next_round();
    fx.ui
        .set_props(badge, (bx::Position(taffy::Position::Relative),));
    fx.mark(badge, DirtyFlags::NEED_BOX_LAYOUT);
    fx.build();
    assert!(fx.built(para));

    // Leaving the flow changes the inline box kind, which the caller marks
    // as a full change.
    fx.next_round();
    fx.ui
        .set_props(badge, (bx::Position(taffy::Position::Absolute),));
    fx.mark(badge, DirtyFlags::NEED_FULL_LAYOUT);
    fx.build();
    assert!(!fx.built(para));

    fx.layout();
    assert_eq!(fx.box_of(para), para_box);
    assert_eq!(fx.box_of(badge), badge_box);
    assert_eq!(
        fx.fragments.boxes[badge_box].box_type,
        BoxFragmentType::OutOfFlow
    );
    assert_eq!(
        fx.inline_text(para),
        "hello ",
        "out-of-flow boxes take no character"
    );
}

#[test]
fn removed_atomics_lose_their_boxes() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = text(&mut ui, "a");
    let first = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    let second = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    ui.append(root, before);
    ui.append(root, first);
    ui.append(root, second);

    let mut fx = Fixture::new(ui, root);
    let second_box = fx.box_of(second);
    let first_item = fx.item(first);

    fx.next_round();
    fx.ui.delete(first);
    fx.mark(root, DirtyFlags::NEED_CHILDREN);
    fx.layout();

    assert!(fx.fragments.box_fragment(first_item).is_none());
    assert_eq!(fx.children(fx.box_of(root)), [second_box]);
    assert_eq!(fx.inline_text(root), "a\u{FFFC}");
}

#[test]
fn nested_inline_formatting_contexts_are_built() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    let section = ui.create(Node::Box, ());
    let inner = paragraph(&mut ui, "deep");
    ui.append(root, outer);
    ui.append(outer, section);
    ui.append(section, inner);

    let fx = Fixture::new(ui, root);

    assert!(fx.built(root));
    assert!(fx.fragments.boxes[fx.box_of(outer)].child_layout.is_block());
    assert!(
        fx.fragments.boxes[fx.box_of(section)]
            .child_layout
            .is_block()
    );
    assert_eq!(fx.inline_text(inner), "deep");
    assert!(fx.valid(fx.box_of(inner)));
}

#[test]
fn unmarked_build_changes_nothing() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = paragraph(&mut ui, "text");
    ui.append(root, para);

    let mut fx = Fixture::new(ui, root);
    let (root_box, para_box) = (fx.box_of(root), fx.box_of(para));

    fx.next_round();
    assert_eq!(fx.build(), root_box);
    assert_eq!(fx.box_of(para), para_box);
    assert!(fx.valid(root_box) && fx.valid(para_box));
    assert!(fx.has_box(para));
}

/// A box whose width is `width`.
fn sized(ui: &mut Ui, width: StyleUnit) -> NodeKey {
    ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(width),
            AutoValue::Auto,
        )),),
    )
}

#[test]
fn box_style_change_restyles_in_place() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = paragraph(&mut ui, "a");
    let b = paragraph(&mut ui, "b");
    ui.append(root, a);
    ui.append(root, b);

    let mut fx = Fixture::new(ui, root);
    let (root_box, a_box, b_box) = (fx.box_of(root), fx.box_of(a), fx.box_of(b));
    let style = fx.fragments.boxes[a_box].style().clone();

    fx.next_round();
    fx.ui.set_props(
        a,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Px(100.0)),
            AutoValue::Auto,
        )),),
    );
    fx.mark(a, DirtyFlags::NEED_BOX_LAYOUT);

    assert_eq!(fx.build(), root_box);
    assert_eq!(fx.box_of(a), a_box, "box changes keep the box");
    assert_ne!(fx.fragments.boxes[a_box].style(), &style);
    assert!(!fx.valid(a_box) && !fx.valid(root_box));
    assert!(fx.valid(b_box));
}

#[test]
fn box_type_follows_style_under_block_parents() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = paragraph(&mut ui, "a");
    ui.append(root, a);

    let mut fx = Fixture::new(ui, root);
    let a_box = fx.box_of(a);
    assert_eq!(fx.fragments.boxes[a_box].box_type, BoxFragmentType::Normal);

    fx.next_round();
    fx.ui.set_props(
        a,
        (bx::Overflow(Point::new(
            taffy::Overflow::Hidden,
            taffy::Overflow::Visible,
        )),),
    );
    fx.mark(a, DirtyFlags::NEED_BOX_LAYOUT);
    fx.build();
    assert_eq!(fx.box_of(a), a_box);
    assert_eq!(
        fx.fragments.boxes[a_box].box_type,
        BoxFragmentType::FlowRoot
    );

    fx.next_round();
    fx.ui
        .set_props(a, (bx::Position(taffy::Position::Absolute),));
    fx.mark(a, DirtyFlags::NEED_FULL_LAYOUT);
    fx.build();
    assert_eq!(
        fx.fragments.boxes[fx.box_of(a)].box_type,
        BoxFragmentType::OutOfFlow
    );
}

#[test]
fn inline_changes_invalidate_their_owner() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let inner = text(&mut ui, "inner");
    let badge = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    let other = paragraph(&mut ui, "other");
    ui.append(root, para);
    ui.append(para, span);
    ui.append(span, inner);
    ui.append(para, badge);
    ui.append(root, other);

    let mut fx = Fixture::new(ui, root);
    let (root_box, para_box, badge_box, other_box) = (
        fx.box_of(root),
        fx.box_of(para),
        fx.box_of(badge),
        fx.box_of(other),
    );

    // An inline element's box style lays out its owner again.
    fx.next_round();
    fx.ui.set_props(
        span,
        (bx::Padding(Rect::new(
            Point::new(StyleUnit::Px(4.0), StyleUnit::Px(0.0)),
            Point::new(StyleUnit::Px(4.0), StyleUnit::Px(0.0)),
        )),),
    );
    fx.mark(span, DirtyFlags::NEED_BOX_LAYOUT);
    fx.build();

    assert_eq!(
        fx.box_of(para),
        para_box,
        "no rebuild without content changes"
    );
    assert!(!fx.valid(para_box) && !fx.valid(root_box));
    assert!(fx.valid(badge_box) && fx.valid(other_box));

    // A position change staying in flow restyles the box inside.
    fx.next_round();
    fx.ui
        .set_props(badge, (bx::Position(taffy::Position::Relative),));
    fx.mark(badge, DirtyFlags::NEED_BOX_LAYOUT);
    fx.build();

    assert_eq!(fx.box_of(para), para_box);
    assert_eq!(fx.box_of(badge), badge_box);
    assert_eq!(
        fx.fragments.boxes[badge_box].box_type,
        BoxFragmentType::AtomicInline
    );
    assert!(!fx.valid(badge_box) && !fx.valid(para_box));
    assert!(fx.valid(other_box));
}

#[test]
fn viewport_changes_invalidate_dependent_boxes() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let wide = sized(&mut ui, StyleUnit::Vw(0.5));
    let fixed = sized(&mut ui, StyleUnit::Px(100.0));
    let para = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let badge = sized(&mut ui, StyleUnit::Vw(0.1));
    ui.set_props(
        badge,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    let plain = ui.create(
        Node::Box,
        (bx::DisplayOuter::Inline, bx::DisplayInner::FlowRoot),
    );
    ui.append(root, wide);
    ui.append(root, fixed);
    ui.append(root, para);
    ui.append(para, span);
    ui.append(span, badge);
    ui.append(para, plain);

    let mut fx = Fixture::new(ui, root);
    let [
        root_box,
        wide_box,
        fixed_box,
        para_box,
        badge_box,
        plain_box,
    ] = [root, wide, fixed, para, badge, plain].map(|node| fx.box_of(node));
    let depends = |fx: &Fixture, key| {
        fx.fragments.boxes[key]
            .bits()
            .contains(NodeLayoutBits::DEPENDS_VIEWPORT)
    };
    assert!(
        depends(&fx, root_box) && depends(&fx, para_box),
        "bits propagate"
    );
    assert!(!depends(&fx, fixed_box) && !depends(&fx, plain_box));

    // Nothing depends on an unchanged viewport.
    fx.next_round();
    fx.build();
    assert!(fx.valid(root_box));

    fx.next_round();
    fx.constraint =
        LayoutConstraint::new_root(Size::new(800.0, 600.0), 16.0, 1.0).with_viewport_changed(true);
    fx.build();

    assert!(!fx.valid(wide_box) && !fx.valid(badge_box));
    assert!(!fx.valid(para_box) && !fx.valid(root_box));
    assert!(fx.valid(fixed_box) && fx.valid(plain_box));
}

/// Horizontal padding of `unit` on both sides.
fn horizontal_padding(unit: StyleUnit) -> bx::Padding {
    let zero = StyleUnit::Px(0.0);
    bx::Padding(Rect::new(Point::new(unit, zero), Point::new(unit, zero)))
}

#[test]
fn inline_elements_on_the_viewport_mark_their_owner() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = ui.create(Node::Box, ());
    let span = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            horizontal_padding(StyleUnit::Vw(0.01)),
        ),
    );
    let inner = text(&mut ui, "inner");
    let other = paragraph(&mut ui, "other");
    ui.append(root, para);
    ui.append(para, span);
    ui.append(span, inner);
    ui.append(root, other);

    let mut fx = Fixture::new(ui, root);
    let [root_box, para_box, other_box] = [root, para, other].map(|node| fx.box_of(node));
    let depends = |fx: &Fixture, key| {
        fx.fragments.boxes[key]
            .bits()
            .contains(NodeLayoutBits::DEPENDS_VIEWPORT)
    };
    assert!(depends(&fx, para_box) && depends(&fx, root_box));
    assert!(!depends(&fx, other_box));

    fx.next_round();
    fx.constraint =
        LayoutConstraint::new_root(Size::new(800.0, 600.0), 16.0, 1.0).with_viewport_changed(true);
    fx.build();

    assert_eq!(fx.box_of(para), para_box, "laid out again without shaping");
    assert!(!fx.valid(para_box) && !fx.valid(root_box));
    assert!(fx.valid(other_box));
}

#[test]
fn inline_element_box_changes_add_viewport_bits() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let para = ui.create(Node::Box, ());
    let span = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            horizontal_padding(StyleUnit::Px(4.0)),
        ),
    );
    let inner = text(&mut ui, "inner");
    ui.append(root, para);
    ui.append(para, span);
    ui.append(span, inner);

    let mut fx = Fixture::new(ui, root);
    let [root_box, para_box] = [root, para].map(|node| fx.box_of(node));
    let depends = |fx: &Fixture, key| {
        fx.fragments.boxes[key]
            .bits()
            .contains(NodeLayoutBits::DEPENDS_VIEWPORT)
    };
    assert!(!depends(&fx, para_box));

    fx.next_round();
    fx.ui
        .set_props(span, (horizontal_padding(StyleUnit::Vw(0.01)),));
    fx.mark(span, DirtyFlags::NEED_BOX_LAYOUT);
    fx.build();

    assert_eq!(fx.box_of(para), para_box);
    assert!(depends(&fx, para_box) && depends(&fx, root_box));

    // The owner's own box changes keep the bits of its inline elements.
    fx.next_round();
    fx.mark(para, DirtyFlags::NEED_BOX_LAYOUT);
    fx.build();
    assert!(depends(&fx, para_box));
}
