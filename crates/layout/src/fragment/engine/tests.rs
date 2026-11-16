use std::sync::Arc;

use comet_tree::{
    node::{Node, NodeKey, Text},
    ui::Ui,
};

use crate::{
    container::{ContainerContext, ContainerLayout},
    fragment::{builder::FragmentTreeBuilder, state::NodeLayoutBits, tree::FragmentTree},
    input::{
        LayoutFlags,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        tree::LayoutInputTree,
    },
    style::{
        LayoutConstraint, bx,
        geometry::{AutoValue, Rect, Size, StyleUnit},
    },
};

const VIEWPORT: Size<f64> = Size::new(400.0, 600.0);

/// A container algorithm that lays out nothing.
struct Empty;

impl ContainerLayout for Empty {
    fn layout(
        &self,
        _: &mut ContainerContext<'_, '_>,
        _: taffy::LayoutInput,
    ) -> taffy::LayoutOutput {
        taffy::LayoutOutput::HIDDEN
    }
}

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

    fn build(&mut self, ui: &Ui, node: NodeKey) {
        let root = self
            .inputs
            .build(ui, &mut self.tree, BuildRequest::new(node, 16.0, VIEWPORT))
            .unwrap();
        self.builder
            .build(
                ui,
                &self.tree,
                root,
                &mut self.fragments,
                &LayoutConstraint::new_root(VIEWPORT, 16.0),
            )
            .unwrap();
        self.tree.clear_flags(root);
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
fn style_units_set_layout_bits_up_to_the_root() {
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
    assert_eq!(f.bits(root), NodeLayoutBits::all());
    assert!(f.bits(plain).is_empty());

    // A box style edit adds the bits of its new style to the ancestors.
    ui.set_props(plain, (width(StyleUnit::Vh(0.5)),));
    f.tree.mark_node(plain, LayoutFlags::NEED_BOX_LAYOUT);
    f.build(&ui, root);

    assert_eq!(f.bits(plain), NodeLayoutBits::DEPENDS_VIEWPORT);
}

#[test]
fn containers_depend_on_everything() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let wrapper = ui.create(Node::Box, ());
    let container = ui.create(Node::Box, (bx::DisplayInner::Container(Arc::new(Empty)),));

    ui.append(root, wrapper);
    ui.append(wrapper, container);

    let mut f = Fixture::new();
    f.build(&ui, root);

    assert_eq!(f.bits(container), NodeLayoutBits::all());
    assert_eq!(f.bits(wrapper), NodeLayoutBits::all());
    assert_eq!(f.bits(root), NodeLayoutBits::all());
}
