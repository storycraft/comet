//! Custom container algorithms laid out by the fragment tree builder.

use comet_content::manager::ContentManager;

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use comet_tree::{
    node::{Node, NodeKey},
    ui::Ui,
};

use super::builder::{Fixture, text};
use crate::{
    container::{ContainerContext, ContainerLayout},
    dirty_map::{DirtyFlags, DirtyMap},
    fragment::{BoxFragmentType, NodeLayoutBits, builder::FragmentTreeBuilder, tree::FragmentTree},
    input::{builder::LayoutInputTreeBuilder, tree::LayoutInputTree},
    style::{LayoutConstraint, bx},
};
use comet_geometry::{AutoValue, Size, StyleUnit};

/// A container algorithm that counts its layouts, sizes itself by its known
/// dimensions, and returns the given bits.
struct Counted(Arc<AtomicU32>, NodeLayoutBits);

impl ContainerLayout for Counted {
    fn layout(
        &self,
        _: &mut ContainerContext<'_, '_>,
        input: taffy::LayoutInput,
    ) -> (taffy::LayoutOutput, NodeLayoutBits) {
        self.0.fetch_add(1, Ordering::Relaxed);
        let output = taffy::LayoutOutput::from_outer_size(taffy::Size {
            width: input.known_dimensions.width.unwrap_or(0.0),
            height: input.known_dimensions.height.unwrap_or(0.0),
        });

        (output, self.1)
    }
}

fn counted(calls: &Arc<AtomicU32>, bits: NodeLayoutBits) -> bx::DisplayInner {
    bx::DisplayInner::Container(Arc::new(Counted(calls.clone(), bits)))
}

fn fixed(width: f64, height: f64) -> bx::Size {
    bx::Size(Size::new(
        AutoValue::Value(StyleUnit::Px(width)),
        AutoValue::Value(StyleUnit::Px(height)),
    ))
}

/// Trees of one root, laid out at a chosen viewport.
#[derive(Default)]
struct Trees {
    inputs: LayoutInputTreeBuilder,
    tree: LayoutInputTree,
    dirty: DirtyMap,
    builder: FragmentTreeBuilder,
    fonts: parley::FontContext,
    fragments: FragmentTree,
}

impl Trees {
    fn layout(&mut self, ui: &Ui, root: NodeKey, width: f64, viewport_changed: bool) {
        let viewport = Size::new(width, 600.0);
        let input_root = self
            .inputs
            .prepare(&mut self.tree, &mut self.dirty)
            .ui(ui)
            .source(root)
            .root_font_size(16.0)
            .viewport(viewport)
            .build()
            .unwrap();
        self.builder
            .prepare(&mut self.fragments, &mut self.dirty)
            .ui(ui)
            .tree(&self.tree)
            .contents(&ContentManager::new())
            .fonts(&mut self.fonts)
            .root(input_root)
            .constraint(
                &LayoutConstraint::new_root(viewport, 16.0, 1.0)
                    .with_viewport_changed(viewport_changed),
            )
            .build()
            .unwrap();
        self.dirty.clear(&self.tree, input_root);
    }

    fn bits(&self, node: NodeKey) -> NodeLayoutBits {
        let key = self
            .fragments
            .box_fragment(self.tree.item_for_node(node).unwrap())
            .unwrap();

        self.fragments.boxes[key].bits()
    }
}

#[test]
fn containers_add_the_bits_their_layout_returns() {
    let calls = Arc::new(AtomicU32::new(0));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let wrapper = ui.create(Node::Box, ());
    let dependent = ui.create(
        Node::Box,
        (counted(&calls, NodeLayoutBits::DEPENDS_VIEWPORT),),
    );
    let independent = ui.create(Node::Box, (counted(&calls, NodeLayoutBits::empty()),));

    ui.append(root, wrapper);
    ui.append(wrapper, dependent);
    ui.append(root, independent);

    let mut trees = Trees::default();
    trees.layout(&ui, root, 400.0, false);

    assert!(calls.load(Ordering::Relaxed) > 0);
    assert_eq!(trees.bits(dependent), NodeLayoutBits::DEPENDS_VIEWPORT);
    assert_eq!(trees.bits(wrapper), NodeLayoutBits::DEPENDS_VIEWPORT);
    assert_eq!(trees.bits(root), NodeLayoutBits::DEPENDS_VIEWPORT);
    assert!(trees.bits(independent).is_empty());
}

#[test]
fn viewport_changes_skip_cached_layouts_of_viewport_dependent_boxes() {
    let calls = Arc::new(AtomicU32::new(0));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    // Fixed size, so the container's layout input never changes.
    let wrapper = ui.create(Node::Box, (fixed(100.0, 50.0),));
    // As if it resolved a length of its own in `vw`.
    let container = ui.create(
        Node::Box,
        (counted(&calls, NodeLayoutBits::DEPENDS_VIEWPORT),),
    );
    ui.append(root, wrapper);
    ui.append(wrapper, container);

    let mut trees = Trees::default();
    let layout = |trees: &mut Trees, width: f64, viewport_changed: bool| {
        trees.layout(&ui, root, width, viewport_changed);
        calls.swap(0, Ordering::Relaxed)
    };

    assert!(layout(&mut trees, 400.0, false) > 0);
    assert_eq!(trees.bits(container), NodeLayoutBits::DEPENDS_VIEWPORT);

    // The cache serves the container while the viewport is not marked changed.
    assert_eq!(layout(&mut trees, 500.0, false), 0);
    // A changed viewport lays out viewport dependent boxes again.
    assert!(layout(&mut trees, 600.0, true) > 0);
}

#[test]
fn percentage_descendants_keep_fixed_size_containers_cached() {
    let calls = Arc::new(AtomicU32::new(0));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    // Fixed size, so it lays out the same whatever the root's size.
    let container = ui.create(
        Node::Box,
        (fixed(100.0, 50.0), counted(&calls, NodeLayoutBits::empty())),
    );
    // Resolves against the container, not the root.
    let child = ui.create(
        Node::Box,
        (bx::Size(Size::new(
            AutoValue::Value(StyleUnit::Percent(0.5)),
            AutoValue::Auto,
        )),),
    );
    ui.append(root, container);
    ui.append(container, child);

    let mut trees = Trees::default();
    let layout = |trees: &mut Trees, width: f64| {
        trees.layout(&ui, root, width, false);
        calls.swap(0, Ordering::Relaxed)
    };

    assert!(layout(&mut trees, 400.0) > 0);
    assert_eq!(trees.bits(child), NodeLayoutBits::DEPENDS_PARENT_SIZE);
    // The child's percentage stays with the child.
    assert!(trees.bits(container).is_empty());
    assert!(trees.bits(root).is_empty());

    // The root is laid out at the new size, but the container is cached.
    assert_eq!(layout(&mut trees, 500.0), 0);
}

#[test]
fn text_children_of_containers_get_anonymous_boxes() {
    let calls = Arc::new(AtomicU32::new(0));
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (counted(&calls, NodeLayoutBits::empty()),));
    let leaf = text(&mut ui, "one");
    ui.append(root, leaf);

    let mut fx = Fixture::new(ui, root);
    let leaf_box = fx.box_of(leaf);
    let box_type = |fx: &Fixture, node| fx.fragments.boxes[fx.box_of(node)].box_type;
    assert_eq!(box_type(&fx, leaf), BoxFragmentType::Anonymous);
    assert!(fx.fragments.boxes[leaf_box].child_layout.is_inline());

    // Reconciled and restyled, the box stays anonymous.
    fx.next_round();
    let other = text(&mut fx.ui, "two");
    fx.ui.append(root, other);
    fx.mark(root, DirtyFlags::NEED_CHILDREN);
    fx.mark(leaf, DirtyFlags::NEED_BOX_LAYOUT);
    fx.layout();

    assert_eq!(fx.box_of(leaf), leaf_box);
    assert_eq!(box_type(&fx, leaf), BoxFragmentType::Anonymous);
    assert_eq!(box_type(&fx, other), BoxFragmentType::Anonymous);
}
