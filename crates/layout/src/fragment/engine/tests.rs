use std::sync::Arc;

use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

use crate::{
    container::{ContainerContext, ContainerLayout},
    dirty_map::DirtyMap,
    fragment::{builder::FragmentTreeBuilder, state::NodeLayoutBits, tree::FragmentTree},
    input::{
        InputFlags,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Rect, Size, StyleUnit},
    },
};

const VIEWPORT: Size<f64> = Size::new(400.0, 600.0);

/// A container algorithm that lays out nothing, and returns the given bits.
struct Empty(NodeLayoutBits);

impl ContainerLayout for Empty {
    fn layout(
        &self,
        _: &mut ContainerContext<'_, '_>,
        _: taffy::LayoutInput,
    ) -> (taffy::LayoutOutput, NodeLayoutBits) {
        (taffy::LayoutOutput::HIDDEN, self.0)
    }
}

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

    fn build(&mut self, ui: &Ui, node: NodeKey) {
        let root = self
            .inputs
            .build(
                ui,
                &mut self.tree,
                &mut self.dirty,
                BuildRequest::new(node, 16.0, VIEWPORT),
            )
            .unwrap();
        self.builder
            .build(
                ui,
                &self.tree,
                &self.dirty,
                root,
                &mut self.fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0),
            )
            .unwrap();
        self.dirty.clear(&self.tree, root);
    }

    fn bits(&self, node: NodeKey) -> NodeLayoutBits {
        let fragment = self
            .fragments
            .box_fragment(self.tree.item_for_node(node).unwrap())
            .unwrap();

        self.fragments.get_state(fragment).unwrap().bits
    }
}

fn width(unit: StyleUnit) -> bx::Size {
    bx::Size(Size::new(AutoValue::Value(unit), AutoValue::Auto))
}

#[test]
fn style_units_set_viewport_bits_up_to_the_root() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let card = ui.create(Node::Box, (width(StyleUnit::Percent(0.5)),));
    let inner = ui.create(
        Node::Box,
        (bx::Padding(Rect::new_all(StyleUnit::Vw(0.01))),),
    );
    let inner_text = ui.create(Node::Text, (Text("inner".into()),));
    let plain = ui.create(Node::Box, (width(StyleUnit::Px(100.0)),));
    let plain_text = ui.create(Node::Text, (Text("plain".into()),));

    ui.append(root, card);
    ui.append(card, inner);
    ui.append(inner, inner_text);
    ui.append(root, plain);
    ui.append(plain, plain_text);

    let mut f = Fixture::new();
    f.build(&ui, root);

    assert_eq!(f.bits(inner), NodeLayoutBits::DEPENDS_VIEWPORT);
    // Its own percentage width, and the viewport padding of its child.
    assert_eq!(f.bits(card), NodeLayoutBits::all());
    // Only the viewport bit reaches the ancestors.
    assert_eq!(f.bits(root), NodeLayoutBits::DEPENDS_VIEWPORT);
    assert!(f.bits(plain).is_empty());

    // A box style edit adds the bits of its new style to the ancestors.
    ui.set_props(plain, (width(StyleUnit::Vh(0.5)),));
    f.dirty
        .mark_node(&f.tree, plain, InputFlags::NEED_BOX_LAYOUT);
    f.build(&ui, root);

    assert_eq!(f.bits(plain), NodeLayoutBits::DEPENDS_VIEWPORT);
}

#[test]
fn containers_add_the_bits_their_layout_returns() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let wrapper = ui.create(Node::Box, ());
    let dependent = ui.create(
        Node::Box,
        (bx::DisplayInner::Container(Arc::new(Empty(
            NodeLayoutBits::DEPENDS_VIEWPORT,
        ))),),
    );
    let independent = ui.create(
        Node::Box,
        (bx::DisplayInner::Container(Arc::new(Empty(
            NodeLayoutBits::empty(),
        ))),),
    );

    ui.append(root, wrapper);
    ui.append(wrapper, dependent);
    ui.append(root, independent);

    let mut f = Fixture::new();
    f.build(&ui, root);

    assert_eq!(f.bits(dependent), NodeLayoutBits::DEPENDS_VIEWPORT);
    assert_eq!(f.bits(wrapper), NodeLayoutBits::DEPENDS_VIEWPORT);
    assert_eq!(f.bits(root), NodeLayoutBits::DEPENDS_VIEWPORT);
    assert!(f.bits(independent).is_empty());
}

/// A container algorithm that counts its layouts, and returns the given
/// bits.
struct Counted(Arc<std::sync::atomic::AtomicU32>, NodeLayoutBits);

impl ContainerLayout for Counted {
    fn layout(
        &self,
        _: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> (taffy::LayoutOutput, NodeLayoutBits) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let output = taffy::LayoutOutput::from_outer_size(taffy::Size {
            width: input.known_dimensions.width.unwrap_or(0.0),
            height: input.known_dimensions.height.unwrap_or(0.0),
        });

        (output, self.1)
    }
}

#[test]
fn viewport_changes_skip_cached_layouts_of_viewport_dependent_boxes() {
    use std::sync::atomic::{AtomicU32, Ordering};

    let calls = Arc::new(AtomicU32::new(0));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    // Fixed size, so the container's layout input never changes.
    let wrapper = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Px(100.0)),
            AutoValue::Value(StyleUnit::Px(50.0)),
        )),),
    );
    let container = ui.create(
        Node::Box,
        // As if it resolved a length of its own in `vw`.
        (bx::DisplayInner::Container(Arc::new(Counted(
            calls.clone(),
            NodeLayoutBits::DEPENDS_VIEWPORT,
        ))),),
    );
    ui.append(root, wrapper);
    ui.append(wrapper, container);

    let mut f = Fixture::new();
    let layout = |f: &mut Fixture, width: f64, viewport_changed: bool| {
        let viewport = Size::new(width, 600.0);
        let input_root = f
            .inputs
            .build(
                &ui,
                &mut f.tree,
                &mut f.dirty,
                BuildRequest::new(root, 16.0, viewport),
            )
            .unwrap();
        f.builder
            .build(
                &ui,
                &f.tree,
                &f.dirty,
                input_root,
                &mut f.fragments,
                &LayoutConstraint::new_root(viewport, 16.0).with_viewport_changed(viewport_changed),
            )
            .unwrap();
        f.dirty.clear(&f.tree, input_root);

        calls.swap(0, Ordering::Relaxed)
    };

    assert!(layout(&mut f, 400.0, false) > 0);
    assert_eq!(f.bits(container), NodeLayoutBits::DEPENDS_VIEWPORT);

    // The cache serves the container while the viewport is not marked changed.
    assert_eq!(layout(&mut f, 500.0, false), 0);
    // A changed viewport lays out viewport dependent boxes again.
    assert!(layout(&mut f, 600.0, true) > 0);
}

#[test]
fn percentage_descendants_keep_fixed_size_ancestors_cached() {
    use std::sync::atomic::{AtomicU32, Ordering};

    let calls = Arc::new(AtomicU32::new(0));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    // Fixed size, so it lays out the same whatever the root's size.
    let container = ui.create(
        Node::Box,
        (
            bx::Size(Size::new(
                AutoValue::Value(StyleUnit::Px(100.0)),
                AutoValue::Value(StyleUnit::Px(50.0)),
            )),
            bx::DisplayInner::Container(Arc::new(Counted(calls.clone(), NodeLayoutBits::empty()))),
        ),
    );
    // Resolves against the container, not the root.
    let child = ui.create(Node::Box, (width(StyleUnit::Percent(0.5)),));
    ui.append(root, container);
    ui.append(container, child);

    let mut f = Fixture::new();
    let layout = |f: &mut Fixture, width: f64| {
        let viewport = Size::new(width, 600.0);
        let input_root = f
            .inputs
            .build(
                &ui,
                &mut f.tree,
                &mut f.dirty,
                BuildRequest::new(root, 16.0, viewport),
            )
            .unwrap();
        f.builder
            .build(
                &ui,
                &f.tree,
                &f.dirty,
                input_root,
                &mut f.fragments,
                &LayoutConstraint::new_root(viewport, 16.0),
            )
            .unwrap();
        f.dirty.clear(&f.tree, input_root);

        calls.swap(0, Ordering::Relaxed)
    };

    assert!(layout(&mut f, 400.0) > 0);
    assert_eq!(f.bits(child), NodeLayoutBits::DEPENDS_PARENT_SIZE);
    // The child's percentage stays with the child.
    assert!(f.bits(container).is_empty());
    assert!(f.bits(root).is_empty());

    // The root is laid out at the new size, but the container is cached.
    assert_eq!(layout(&mut f, 500.0), 0);
}

#[test]
fn viewport_changes_resolve_viewport_dependent_styles_again() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let half = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Vw(0.5)),
            AutoValue::Value(StyleUnit::Px(10.0)),
        )),),
    );
    ui.append(root, half);

    let mut f = Fixture::new();
    let layout = |f: &mut Fixture, width: f64, viewport_changed: bool| {
        let viewport = Size::new(width, 600.0);
        let input_root = f
            .inputs
            .build(
                &ui,
                &mut f.tree,
                &mut f.dirty,
                BuildRequest::new(root, 16.0, viewport),
            )
            .unwrap();
        f.builder
            .build(
                &ui,
                &f.tree,
                &f.dirty,
                input_root,
                &mut f.fragments,
                &LayoutConstraint::new_root(viewport, 16.0).with_viewport_changed(viewport_changed),
            )
            .unwrap();
        f.dirty.clear(&f.tree, input_root);

        let fragment = f
            .fragments
            .box_fragment(f.tree.item_for_node(half).unwrap())
            .unwrap();
        f.fragments.items[fragment].layout.size.width
    };

    assert_eq!(layout(&mut f, 400.0, false), 200.0);
    // Unmarked, the style keeps the width resolved with the old viewport.
    assert_eq!(layout(&mut f, 600.0, false), 200.0);
    assert_eq!(layout(&mut f, 800.0, true), 400.0);
}

#[test]
fn viewport_changes_reach_boxes_inside_inline_elements() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let before = ui.create(Node::Text, (Text("before ".into()),));
    let span = ui.create(Node::Box, (bx::DisplayOuter::Inline,));
    let badge = ui.create(
        Node::Box,
        (
            bx::DisplayOuter::Inline,
            bx::DisplayInner::FlowRoot,
            bx::Size(Size::new(
                AutoValue::Value(StyleUnit::Vw(0.1)),
                AutoValue::Value(StyleUnit::Px(10.0)),
            )),
        ),
    );
    ui.append(root, before);
    ui.append(root, span);
    ui.append(span, badge);

    let mut f = Fixture::new();
    let layout = |f: &mut Fixture, width: f64, viewport_changed: bool| {
        let viewport = Size::new(width, 600.0);
        let input_root = f
            .inputs
            .build(
                &ui,
                &mut f.tree,
                &mut f.dirty,
                BuildRequest::new(root, 16.0, viewport),
            )
            .unwrap();
        f.builder
            .build(
                &ui,
                &f.tree,
                &f.dirty,
                input_root,
                &mut f.fragments,
                &LayoutConstraint::new_root(viewport, 16.0).with_viewport_changed(viewport_changed),
            )
            .unwrap();
        f.dirty.clear(&f.tree, input_root);

        let fragment = f
            .fragments
            .box_fragment(f.tree.item_for_node(badge).unwrap())
            .unwrap();
        f.fragments.items[fragment].layout.size.width
    };

    assert_eq!(layout(&mut f, 400.0, false), 40.0);
    // The span between the root and the badge is not flagged.
    assert_eq!(layout(&mut f, 800.0, true), 80.0);
}

/// Lay out `root` in a viewport of `width`, without flagging anything.
fn layout_at(f: &mut Fixture, ui: &Ui, root: NodeKey, width: f64) {
    let viewport = Size::new(width, 600.0);
    let input_root = f
        .inputs
        .build(
            ui,
            &mut f.tree,
            &mut f.dirty,
            BuildRequest::new(root, 16.0, viewport),
        )
        .unwrap();
    f.builder
        .build(
            ui,
            &f.tree,
            &f.dirty,
            input_root,
            &mut f.fragments,
            &LayoutConstraint::new_root(viewport, 16.0),
        )
        .unwrap();
    f.dirty.clear(&f.tree, input_root);
}

#[test]
fn fixed_size_boxes_reuse_their_layout_when_the_parent_size_changes() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let fixed = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Px(300.0)),
            AutoValue::Value(StyleUnit::Px(100.0)),
        )),),
    );
    let text = ui.create(Node::Text, (Text("kept lines".into()),));
    ui.append(root, fixed);
    ui.append(fixed, text);

    let mut f = Fixture::new();
    layout_at(&mut f, &ui, root, 400.0);
    let fixed_fragment = f
        .fragments
        .box_fragment(f.tree.item_for_node(fixed).unwrap())
        .unwrap();
    let lines: Vec<_> = f.fragments.items.children(fixed_fragment).collect();
    assert!(!lines.is_empty());

    // The root is laid out in the new viewport, but the fixed box is served
    // from its cache, so its lines are not emitted again.
    layout_at(&mut f, &ui, root, 500.0);

    assert_eq!(
        f.fragments
            .items
            .children(fixed_fragment)
            .collect::<Vec<_>>(),
        lines
    );
    assert_eq!(
        f.fragments.items[fixed_fragment].layout.size,
        Size::new(300.0, 100.0)
    );
}

#[test]
fn boxes_depending_on_the_parent_size_lay_out_again() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let half = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Percent(0.5)),
            AutoValue::Value(StyleUnit::Px(100.0)),
        )),),
    );
    ui.append(root, half);

    let mut f = Fixture::new();
    let width = |f: &Fixture| {
        let fragment = f
            .fragments
            .box_fragment(f.tree.item_for_node(half).unwrap())
            .unwrap();
        f.fragments.items[fragment].layout.size.width
    };

    layout_at(&mut f, &ui, root, 400.0);
    assert_eq!(width(&f), 200.0);

    layout_at(&mut f, &ui, root, 600.0);
    assert_eq!(width(&f), 300.0);
}

#[test]
fn builds_list_the_fragments_they_changed() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let fixed = |width, height| {
        bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Px(width)),
            AutoValue::Value(StyleUnit::Px(height)),
        ))
    };
    let a = ui.create(Node::Box, (fixed(100.0, 50.0),));
    let b = ui.create(Node::Box, (fixed(100.0, 50.0),));
    let c = ui.create(Node::Box, (fixed(100.0, 50.0),));
    let paragraph = ui.create(Node::Box, ());
    let text = ui.create(
        Node::Text,
        (Text("lines wrap again at every width".into()),),
    );
    ui.append(root, a);
    ui.append(root, b);
    ui.append(root, c);
    ui.append(root, paragraph);
    ui.append(paragraph, text);

    let mut f = Fixture::new();
    let box_of = |f: &Fixture, node| {
        f.fragments
            .box_fragment(f.tree.item_for_node(node).unwrap())
            .unwrap()
    };
    // Read and clear the changed fragments, as the next step does.
    let changed = |f: &mut Fixture| {
        let changed = f.fragments.changed().to_vec();
        f.fragments.clear_changed();
        let mut unique = changed.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), changed.len(), "listed twice: {changed:?}");
        for &key in &changed {
            assert!(f.fragments.items.get(key).is_some(), "deleted: {key:?}");
        }

        changed
    };

    // The first build creates every fragment.
    layout_at(&mut f, &ui, root, 400.0);
    let listed = changed(&mut f);
    let root_fragment = box_of(&f, root);
    assert_eq!(
        listed.len(),
        f.fragments.items.descendants(root_fragment).count()
    );

    // A narrower viewport resizes the root and the paragraph, and replaces
    // its lines, but leaves the fixed boxes in place.
    layout_at(&mut f, &ui, root, 120.0);
    let listed = changed(&mut f);
    assert!(listed.contains(&root_fragment));
    assert!(listed.contains(&box_of(&f, paragraph)));
    for node in [a, b, c] {
        assert!(!listed.contains(&box_of(&f, node)));
    }
    let paragraph_fragment = box_of(&f, paragraph);
    for key in f.fragments.items.descendants(paragraph_fragment).skip(1) {
        assert!(listed.contains(&key), "replaced line part not listed");
    }

    // Resizing the first box moves the boxes after it, unmarked.
    ui.set_props(a, (fixed(100.0, 80.0),));
    f.dirty.mark_node(&f.tree, a, InputFlags::NEED_BOX_LAYOUT);
    layout_at(&mut f, &ui, root, 120.0);
    let listed = changed(&mut f);
    for node in [root, a, b, c, paragraph] {
        assert!(listed.contains(&box_of(&f, node)), "{node:?} not listed");
    }
    // Moving the paragraph keeps its lines, relative to it.
    for key in f.fragments.items.descendants(paragraph_fragment).skip(1) {
        assert!(!listed.contains(&key));
    }

    // A build without changes lists nothing.
    layout_at(&mut f, &ui, root, 120.0);
    assert!(changed(&mut f).is_empty());

    // Changing a box's style to the same size lists nothing either.
    f.dirty.mark_node(&f.tree, b, InputFlags::NEED_BOX_LAYOUT);
    layout_at(&mut f, &ui, root, 120.0);
    assert!(changed(&mut f).is_empty());

    // Builds add to the list until it is cleared.
    ui.set_props(b, (fixed(100.0, 60.0),));
    f.dirty.mark_node(&f.tree, b, InputFlags::NEED_BOX_LAYOUT);
    layout_at(&mut f, &ui, root, 120.0);
    ui.set_props(a, (fixed(110.0, 80.0),));
    f.dirty.mark_node(&f.tree, a, InputFlags::NEED_BOX_LAYOUT);
    layout_at(&mut f, &ui, root, 120.0);
    let listed = changed(&mut f);
    assert!(listed.contains(&box_of(&f, a)));
    assert!(listed.contains(&box_of(&f, b)));
    assert!(changed(&mut f).is_empty());
}

#[test]
fn builds_of_a_removed_root_list_nothing() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());

    let mut f = Fixture::new();
    layout_at(&mut f, &ui, root, 400.0);
    assert!(!f.fragments.changed().is_empty());
    let input_root = f.tree.item_for_node(root).unwrap();

    f.tree.delete_item(input_root);
    let built = f.builder.build(
        &ui,
        &f.tree,
        &f.dirty,
        input_root,
        &mut f.fragments,
        &LayoutConstraint::new_root(VIEWPORT, 16.0),
    );
    assert!(built.is_none());
    assert!(f.fragments.changed().is_empty());
}
