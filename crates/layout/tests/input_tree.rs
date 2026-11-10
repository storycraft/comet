use std::sync::Arc;

use comet_layout::{
    input::{
        ChildLayout, LayoutFlags, LayoutInputKey, LayoutInputKind, builder::LayoutInputTreeBuilder,
        tree::LayoutInputTree,
    },
    style::{
        bx::{self, DisplayInner, DisplayOuter},
        geometry::{Size, StyleUnit},
    },
};
use comet_tree::{node::Node, ui::Ui};

const VIEWPORT: Size<f64> = Size::new(800.0, 600.0);

fn build(
    builder: &mut LayoutInputTreeBuilder,
    ui: &Ui,
    root: comet_tree::node::NodeKey,
    tree: &mut LayoutInputTree,
    target: Option<LayoutInputKey>,
) -> Option<LayoutInputKey> {
    builder.build(ui, root, 16.0, VIEWPORT, tree, target)
}

fn clean(tree: &mut LayoutInputTree, root: LayoutInputKey) {
    let keys: Vec<_> = tree.items.descendants(root).collect();

    for key in keys {
        tree.items[key].flags = LayoutFlags::empty();
    }
}

fn shape(
    tree: &LayoutInputTree,
    root: LayoutInputKey,
) -> Vec<(comet_tree::node::NodeKey, LayoutInputKind, ChildLayout, f32)> {
    tree.items
        .descendants(root)
        .map(|key| {
            let item = &tree.items[key];
            (
                item.node,
                item.kind,
                item.child_layout,
                item.text_style.font_size,
            )
        })
        .collect()
}

#[test]
fn fresh_tree_mirrors_ui_with_all_flags() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let text = ui.create(Node::Text, ());
    let block = ui.create(Node::Box, (DisplayOuter::Block,));
    let content = ui.create(Node::Box, (DisplayInner::Content,));
    let hidden = ui.create(Node::Text, ());
    let atomic = ui.create(Node::Box, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    let atomic_text = ui.create(Node::Text, ());

    ui.append(root, span);
    ui.append(span, text);
    ui.append(span, block);
    ui.append(root, content);
    ui.append(content, hidden);
    ui.append(root, atomic);
    ui.append(atomic, atomic_text);

    let mut tree = LayoutInputTree::new();
    let key = build(
        &mut LayoutInputTreeBuilder::new(),
        &ui,
        root,
        &mut tree,
        None,
    )
    .unwrap();

    assert_eq!(
        shape(&tree, key),
        vec![
            (root, LayoutInputKind::Block, ChildLayout::Block, 16.0),
            (span, LayoutInputKind::Inline, ChildLayout::Inline, 16.0),
            (text, LayoutInputKind::Text, ChildLayout::None, 16.0),
            (block, LayoutInputKind::Block, ChildLayout::Block, 16.0),
            (content, LayoutInputKind::Block, ChildLayout::None, 16.0),
            (
                atomic,
                LayoutInputKind::AtomicInline,
                ChildLayout::Block,
                16.0
            ),
            (atomic_text, LayoutInputKind::Text, ChildLayout::None, 16.0),
        ]
    );

    for item in tree.items.descendants(key) {
        assert_eq!(tree.items[item].flags, LayoutFlags::all());
        assert_eq!(tree.item_for_node(tree.items[item].node), Some(item));
    }

    assert_eq!(tree.item_for_node(hidden), None);
}

#[test]
fn text_styles_resolve_units_and_share_allocations() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (bx::FontSize(StyleUnit::Em(1.25)),));
    let inherited = ui.create(Node::Box, ());
    let changed = ui.create(
        Node::Box,
        (
            bx::FontSize(StyleUnit::Percent(1.5)),
            bx::LineHeight(StyleUnit::Percent(2.0)),
            bx::LetterSpacing(StyleUnit::Em(0.1)),
        ),
    );
    let rem = ui.create(Node::Text, (bx::FontSize(StyleUnit::Rem(2.0)),));
    let vw = ui.create(Node::Text, (bx::FontSize(StyleUnit::Vw(0.01)),));

    ui.append(root, inherited);
    ui.append(root, changed);
    ui.append(changed, rem);
    ui.append(changed, vw);

    let mut tree = LayoutInputTree::new();
    let key = build(
        &mut LayoutInputTreeBuilder::new(),
        &ui,
        root,
        &mut tree,
        None,
    )
    .unwrap();
    let style = |node| {
        tree.items[tree.item_for_node(node).unwrap()]
            .text_style
            .clone()
    };

    assert_eq!(style(root).font_size, 20.0);
    assert!(Arc::ptr_eq(&tree.items[key].text_style, &style(inherited)));

    let changed = style(changed);
    assert_eq!(changed.font_size, 30.0);
    assert_eq!(changed.line_height, parley::LineHeight::Absolute(60.0));
    assert_eq!(changed.letter_spacing, 3.0);
    assert_eq!(style(rem).font_size, 32.0);
    assert_eq!(style(vw).font_size, 8.0);
}

#[test]
fn mark_dirty_propagates_to_ancestors() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let left = ui.create(Node::Box, ());
    let right = ui.create(Node::Box, ());
    let leaf = ui.create(Node::Text, ());

    ui.append(root, left);
    ui.append(root, right);
    ui.append(left, leaf);

    let mut tree = LayoutInputTree::new();
    let key = build(
        &mut LayoutInputTreeBuilder::new(),
        &ui,
        root,
        &mut tree,
        None,
    )
    .unwrap();
    clean(&mut tree, key);

    let leaf_key = tree.item_for_node(leaf).unwrap();
    tree.mark_dirty(leaf_key);

    assert_eq!(tree.items[leaf_key].flags, LayoutFlags::DIRTY);
    assert_eq!(
        tree.items[tree.item_for_node(left).unwrap()].flags,
        LayoutFlags::CHILDREN_DIRTY
    );
    assert_eq!(tree.items[key].flags, LayoutFlags::CHILDREN_DIRTY);
    assert!(
        tree.items[tree.item_for_node(right).unwrap()]
            .flags
            .is_empty()
    );

    tree.mark_node_dirty(right);
    assert_eq!(
        tree.items[tree.item_for_node(right).unwrap()].flags,
        LayoutFlags::DIRTY
    );

    // Unmapped nodes are ignored.
    let unmapped = ui.create(Node::Box, ());
    tree.mark_node_dirty(unmapped);
}

#[test]
fn partial_update_reconciles_children_and_reuses_clean_subtrees() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = ui.create(Node::Box, ());
    let b = ui.create(Node::Box, ());
    let c = ui.create(Node::Text, ());
    let inner = ui.create(Node::Box, ());

    ui.append(root, a);
    ui.append(root, b);
    ui.append(root, c);
    ui.append(b, inner);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, None).unwrap();
    clean(&mut tree, key);

    // Clean targets return without reading the UI.
    assert_eq!(
        build(&mut builder, &ui, root, &mut tree, Some(key)),
        Some(key)
    );

    let a_key = tree.item_for_node(a).unwrap();
    let b_key = tree.item_for_node(b).unwrap();
    let inner_key = tree.item_for_node(inner).unwrap();

    // Reorder, delete, and insert.
    let d = ui.create(Node::Text, ());
    ui.before(a, b);
    ui.delete(c);
    ui.append(root, d);
    tree.mark_dirty(key);

    // A mutable property borrow would panic if the builder inspected this branch.
    let borrowed = ui.prop_mut::<DisplayInner>(inner);
    assert_eq!(
        build(&mut builder, &ui, root, &mut tree, Some(key)),
        Some(key)
    );
    drop(borrowed);

    let children: Vec<_> = tree.items.children(key).collect();
    let d_key = tree.item_for_node(d).unwrap();

    assert_eq!(children, vec![b_key, a_key, d_key]);
    assert_eq!(tree.item_for_node(c), None);
    assert_eq!(tree.item_for_node(inner), Some(inner_key));
    assert_eq!(tree.items[d_key].flags, LayoutFlags::all());

    // Clean children are kept without being marked; only the reordered parent is.
    assert!(tree.items[a_key].flags.is_empty());
    assert!(tree.items[b_key].flags.is_empty());
    assert!(tree.items[inner_key].flags.is_empty());
    assert_eq!(
        tree.items[key].flags,
        LayoutFlags::DIRTY | LayoutFlags::CHILDREN_DIRTY
    );
}

#[test]
fn changed_text_style_updates_descendants() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, ());
    let fixed = ui.create(Node::Text, (bx::FontSize(StyleUnit::Px(10.0)),));

    ui.append(root, child);
    ui.append(child, text);
    ui.append(child, fixed);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, None).unwrap();
    clean(&mut tree, key);

    ui.set_props(root, (bx::FontSize(StyleUnit::Px(24.0)),));
    tree.mark_dirty(key);
    build(&mut builder, &ui, root, &mut tree, Some(key)).unwrap();

    let item = |node| &tree.items[tree.item_for_node(node).unwrap()];

    assert_eq!(item(text).text_style.font_size, 24.0);
    assert_eq!(item(text).flags, LayoutFlags::DIRTY);
    assert_eq!(
        item(child).flags,
        LayoutFlags::DIRTY | LayoutFlags::CHILDREN_DIRTY
    );
    // Its own font size wins, so the resolved style is unchanged and it stays clean.
    assert_eq!(item(fixed).text_style.font_size, 10.0);
    assert!(item(fixed).flags.is_empty());

    let mut fresh = LayoutInputTree::new();
    let fresh_key = build(&mut builder, &ui, root, &mut fresh, None).unwrap();
    assert_eq!(shape(&tree, key), shape(&fresh, fresh_key));
}

#[test]
fn partial_update_of_descendant_uses_parent_style() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (bx::FontSize(StyleUnit::Px(20.0)),));
    let child = ui.create(Node::Box, ());
    ui.append(root, child);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, None).unwrap();
    clean(&mut tree, key);

    let child_key = tree.item_for_node(child).unwrap();
    ui.set_props(child, (bx::FontSize(StyleUnit::Em(2.0)),));
    tree.mark_dirty(child_key);

    assert_eq!(
        build(&mut builder, &ui, child, &mut tree, Some(child_key)),
        Some(child_key)
    );
    assert_eq!(tree.items[child_key].text_style.font_size, 40.0);

    // A target generated for another node is replaced by a new item in place.
    let other_child = ui.create(Node::Text, ());
    ui.append(root, other_child);
    let replaced = build(&mut builder, &ui, other_child, &mut tree, Some(child_key)).unwrap();
    assert_ne!(replaced, child_key);
    assert!(tree.items.get(child_key).is_none());
    assert_eq!(tree.items.parent(replaced), Some(key));
    assert_eq!(tree.item_for_node(child), None);
    assert_eq!(tree.item_for_node(other_child), Some(replaced));
    assert_eq!(tree.items[replaced].flags, LayoutFlags::all());

    // A descendant root without a parent item resolves its UI ancestors.
    let mut other = LayoutInputTree::new();
    let other_key = build(&mut builder, &ui, child, &mut other, None).unwrap();
    assert_eq!(other.items[other_key].text_style.font_size, 40.0);
}

#[test]
fn rebuilding_a_node_only_updates_its_own_item() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let paragraph = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let text = ui.create(Node::Text, ());
    let interruption = ui.create(Node::Box, ());

    ui.append(root, paragraph);
    ui.append(paragraph, span);
    ui.append(span, text);
    ui.append(span, interruption);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, None).unwrap();
    let paragraph_key = tree.item_for_node(paragraph).unwrap();
    let span_key = tree.item_for_node(span).unwrap();
    let interruption_key = tree.item_for_node(interruption).unwrap();

    // Child layouts come from each node's own display, not from its content.
    assert_eq!(tree.items[paragraph_key].child_layout, ChildLayout::Block);
    assert_eq!(tree.items[span_key].child_layout, ChildLayout::Inline);
    clean(&mut tree, key);

    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    tree.mark_node_dirty(interruption);
    assert_eq!(
        build(
            &mut builder,
            &ui,
            interruption,
            &mut tree,
            Some(interruption_key)
        ),
        Some(interruption_key)
    );

    assert_eq!(
        tree.items[interruption_key].kind,
        LayoutInputKind::AtomicInline
    );
    assert_eq!(
        tree.items[interruption_key].child_layout,
        ChildLayout::Block
    );
    assert_eq!(tree.items[paragraph_key].child_layout, ChildLayout::Block);
    assert_eq!(tree.items[paragraph_key].flags, LayoutFlags::CHILDREN_DIRTY);
    assert_eq!(tree.items[span_key].flags, LayoutFlags::CHILDREN_DIRTY);
    assert_fresh(&ui, root, &tree, key);

    // Deleting a node flags its parent's child list.
    clean(&mut tree, key);
    ui.delete(interruption);
    assert_eq!(
        build(
            &mut builder,
            &ui,
            interruption,
            &mut tree,
            Some(interruption_key)
        ),
        None
    );
    assert_eq!(tree.item_for_node(interruption), None);
    assert_eq!(tree.items[span_key].flags, LayoutFlags::DIRTY);
    assert_eq!(tree.items[paragraph_key].flags, LayoutFlags::CHILDREN_DIRTY);
    assert_fresh(&ui, root, &tree, key);
}

fn assert_fresh(
    ui: &Ui,
    root: comet_tree::node::NodeKey,
    tree: &LayoutInputTree,
    key: LayoutInputKey,
) {
    let mut fresh = LayoutInputTree::new();
    let fresh_key = build(
        &mut LayoutInputTreeBuilder::new(),
        ui,
        root,
        &mut fresh,
        None,
    )
    .unwrap();
    assert_eq!(shape(tree, key), shape(&fresh, fresh_key));
}
