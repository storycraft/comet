use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

use crate::{
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::state::NodeLayoutBits,
    fragment2::{BoxFragmentKey, BoxFragmentType, FragmentTree, FragmentTreeBuilder},
    input::{
        LayoutInputKey,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Point, Rect, Size, StyleUnit},
    },
};

pub(super) const VIEWPORT: Size<f64> = Size::new(400.0, 600.0);
const CONSTRAINT: LayoutConstraint = LayoutConstraint::new_root(VIEWPORT, 16.0);

pub(super) struct Fixture {
    pub(super) ui: Ui,
    pub(super) root: NodeKey,
    pub(super) tree: LayoutInputTree,
    pub(super) dirty: DirtyMap,
    pub(super) input: LayoutInputTreeBuilder,
    pub(super) builder: FragmentTreeBuilder,
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
            fragments: FragmentTree::new(),
            layout_root: None,
            root_box: None,
            constraint: CONSTRAINT,
        };
        fixture.build();

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
                &mut self.dirty,
                layout_root,
                &mut self.fragments,
                &self.constraint,
            )
        } else {
            self.builder.update(
                &self.ui,
                &self.tree,
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

    /// Every box in the tree is the box of its source, the boxes inside each
    /// inline layout are its children, and the box count matches.
    fn check(&self, root: BoxFragmentKey) {
        let mut count = 0;

        for key in self.fragments.boxes.descendants(root) {
            count += 1;
            let fragment = &self.fragments.boxes[key];
            assert_eq!(self.fragments.box_fragment(fragment.source), Some(key));

            if let Some(inline) = fragment.inline() {
                let mut atomics: Vec<_> = inline.atomics.values().copied().collect();
                let mut children = self.children(key);
                atomics.sort();
                children.sort();
                assert_eq!(atomics, children);
            }
        }

        assert_eq!(count, self.fragments.boxes.len());
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
    let content = ui.create(Node::Box, (bx::DisplayInner::Content,));

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
fn text_change_rebuilds_its_inline_formatting_context() {
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

    let new_para = fx.box_of(para);
    assert_ne!(
        new_para, para_box,
        "the inline formatting context is rebuilt"
    );
    assert_eq!(
        fx.fragments.boxes[new_para].inline().unwrap().text,
        "goodbye \u{FFFC}"
    );
    assert_eq!(fx.children(root_box), [new_para, other_box]);
    assert_eq!(fx.box_of(badge), badge_box, "boxes inside are reused");
    assert_eq!(fx.children(new_para), [badge_box]);
    assert!(fx.valid(badge_box), "reused boxes keep their layout");
    assert!(fx.valid(other_box));
    assert!(!fx.valid(root_box));
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

    assert_eq!(fx.build(), root_box, "the owner is not rebuilt");
    let new_badge = fx.box_of(badge);
    assert_ne!(new_badge, badge_box);
    assert_eq!(
        fx.fragments.boxes[new_badge].inline().unwrap().text,
        "changed"
    );
    assert_eq!(
        fx.fragments.boxes[new_badge].box_type,
        BoxFragmentType::AtomicInline
    );
    assert_eq!(fx.children(root_box), [new_badge]);
    assert!(!fx.valid(root_box));
}

#[test]
fn root_inline_rebuild_returns_the_new_root_box() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let leaf = text(&mut ui, "one");
    ui.append(root, leaf);

    let mut fx = Fixture::new(ui, root);
    let root_box = fx.box_of(root);

    fx.next_round();
    fx.ui.set_props(leaf, (Text("two".into()),));
    fx.mark(leaf, DirtyFlags::NEED_FULL_LAYOUT);

    let new_root = fx.build();
    assert_ne!(new_root, root_box);
    assert_eq!(fx.box_of(root), new_root);
    assert_eq!(fx.fragments.boxes[new_root].inline().unwrap().text, "two");
}

#[test]
fn child_layout_kind_change_rebuilds_the_box() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let section = ui.create(Node::Box, ());
    let inner = paragraph(&mut ui, "inner");
    ui.append(root, section);
    ui.append(section, inner);

    let mut fx = Fixture::new(ui, root);
    let section_box = fx.box_of(section);
    assert!(fx.fragments.boxes[section_box].child_layout.is_block());

    fx.next_round();
    let leading = text(&mut fx.ui, "leading");
    fx.ui.prepend(section, leading);
    fx.mark(section, DirtyFlags::NEED_CHILDREN);
    fx.build();

    let new_section = fx.box_of(section);
    assert_ne!(new_section, section_box);
    assert!(fx.fragments.boxes[new_section].child_layout.is_inline());
    assert_eq!(fx.children(fx.box_of(root)), [new_section]);
    assert_eq!(
        fx.fragments.boxes[fx.box_of(inner)].box_type,
        BoxFragmentType::InlinePart
    );
}

#[test]
fn flow_change_inside_inline_content_rebuilds_the_owner() {
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

    // Staying in flow leaves the owner alone.
    fx.next_round();
    fx.ui
        .set_props(badge, (bx::Position(taffy::Position::Relative),));
    fx.mark(badge, DirtyFlags::NEED_BOX_LAYOUT);
    fx.build();
    assert_eq!(fx.box_of(para), para_box);

    // Leaving the flow changes the inline box kind, which the caller marks
    // as a full change.
    fx.next_round();
    fx.ui
        .set_props(badge, (bx::Position(taffy::Position::Absolute),));
    fx.mark(badge, DirtyFlags::NEED_FULL_LAYOUT);
    fx.build();

    let new_para = fx.box_of(para);
    assert_ne!(new_para, para_box);
    assert_eq!(fx.box_of(badge), badge_box);
    assert_eq!(
        fx.fragments.boxes[badge_box].box_type,
        BoxFragmentType::OutOfFlow
    );
    assert_eq!(
        fx.fragments.boxes[new_para].inline().unwrap().text,
        "hello ",
        "out-of-flow boxes take no character"
    );
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
        LayoutConstraint::new_root(Size::new(800.0, 600.0), 16.0).with_viewport_changed(true);
    fx.build();

    assert!(!fx.valid(wide_box) && !fx.valid(badge_box));
    assert!(!fx.valid(para_box) && !fx.valid(root_box));
    assert!(fx.valid(fixed_box) && fx.valid(plain_box));
}
