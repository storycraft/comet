use std::sync::Arc;

use comet_layout::{
    input::{
        ChildLayout, InputFlags, LayoutInputKey, LayoutInputKind,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        text::TextStyleBits,
        tree::LayoutInputTree,
    },
    style::{
        bx::{self, DisplayInner, DisplayOuter},
        geometry::{Size, StyleUnit},
    },
};
use comet_tree::{node::Node, ui::Ui};

const VIEWPORT: Size<f64> = Size::new(800.0, 600.0);
const FULL: InputFlags = InputFlags::NEED_FULL_LAYOUT;
const CHILDREN: InputFlags = InputFlags::NEED_CHILDREN;

fn build(
    builder: &mut LayoutInputTreeBuilder,
    ui: &Ui,
    root: comet_tree::node::NodeKey,
    tree: &mut LayoutInputTree,
) -> Option<LayoutInputKey> {
    builder.build(ui, tree, BuildRequest::new(root, 16.0, VIEWPORT))
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
                item.text_style.span.font_size,
            )
        })
        .collect()
}

fn assert_fresh(
    ui: &Ui,
    root: comet_tree::node::NodeKey,
    tree: &LayoutInputTree,
    key: LayoutInputKey,
) {
    let mut fresh = LayoutInputTree::new();
    let fresh_key = build(&mut LayoutInputTreeBuilder::new(), ui, root, &mut fresh).unwrap();
    assert_eq!(shape(tree, key), shape(&fresh, fresh_key));
}

fn assert_clean(tree: &LayoutInputTree, root: LayoutInputKey) {
    for key in tree.items.descendants(root) {
        assert!(tree.items[key].flags.is_empty());
        assert!(tree.items[key].child_flags.is_empty());
    }
}

fn flags2(tree: &LayoutInputTree, node: comet_tree::node::NodeKey) -> InputFlags {
    tree.items[tree.item_for_node(node).unwrap()].flags
}

fn child_flags(tree: &LayoutInputTree, node: comet_tree::node::NodeKey) -> InputFlags {
    tree.items[tree.item_for_node(node).unwrap()].child_flags
}

#[test]
fn fresh_tree_mirrors_ui_with_full_layout_flags() {
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
    let key = build(&mut LayoutInputTreeBuilder::new(), &ui, root, &mut tree).unwrap();

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
        assert_eq!(tree.items[item].flags, FULL);
        assert_eq!(tree.item_for_node(tree.items[item].node), Some(item));
    }

    // New items are created with full flags, leaves included.
    for item in tree.items.descendants(key) {
        assert_eq!(tree.items[item].child_flags, FULL);
    }
    assert_eq!(child_flags(&tree, text), FULL);

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
    let key = build(&mut LayoutInputTreeBuilder::new(), &ui, root, &mut tree).unwrap();
    let style = |node| {
        tree.items[tree.item_for_node(node).unwrap()]
            .text_style
            .clone()
    };

    assert_eq!(style(root).span.font_size, 20.0);
    assert!(Arc::ptr_eq(&tree.items[key].text_style, &style(inherited)));

    let changed = style(changed);
    assert_eq!(changed.span.font_size, 30.0);
    assert_eq!(changed.span.line_height, parley::LineHeight::Absolute(60.0));
    assert_eq!(changed.span.letter_spacing, 3.0);
    assert_eq!(style(rem).span.font_size, 32.0);
    assert_eq!(style(vw).span.font_size, 8.0);
}

#[test]
fn style_units_report_viewport_dependence() {
    for unit in [StyleUnit::Vw(0.5), StyleUnit::Vh(0.5)] {
        assert!(unit.is_viewport_relative());
    }

    for unit in [
        StyleUnit::Px(1.0),
        StyleUnit::Percent(0.5),
        StyleUnit::Em(1.0),
        StyleUnit::Rem(1.0),
    ] {
        assert!(!unit.is_viewport_relative());
    }
}

#[test]
fn viewport_lengths_mark_text_styles_and_their_descendants() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (bx::FontSize(StyleUnit::Rem(1.5)),));
    let spaced = ui.create(Node::Box, (bx::LetterSpacing(StyleUnit::Vw(0.01)),));
    let inherited = ui.create(Node::Text, ());
    let overridden = ui.create(Node::Text, (bx::FontSize(StyleUnit::Px(12.0)),));
    let tall = ui.create(Node::Box, (bx::LineHeight(StyleUnit::Vh(0.05)),));
    let plain = ui.create(
        Node::Box,
        (
            bx::FontSize(StyleUnit::Em(2.0)),
            bx::WordSpacing(StyleUnit::Percent(0.5)),
        ),
    );

    ui.append(root, spaced);
    ui.append(spaced, inherited);
    ui.append(spaced, overridden);
    ui.append(root, tall);
    ui.append(root, plain);

    let mut tree = LayoutInputTree::new();
    let key = build(&mut LayoutInputTreeBuilder::new(), &ui, root, &mut tree).unwrap();
    let bits = |tree: &LayoutInputTree, node| {
        tree.items[tree.item_for_node(node).unwrap()]
            .text_style
            .bits
    };

    assert_eq!(bits(&tree, spaced), TextStyleBits::DEPENDS_VIEWPORT);
    assert_eq!(bits(&tree, tall), TextStyleBits::DEPENDS_VIEWPORT);
    // Descendants inherit values resolved with the viewport.
    assert_eq!(bits(&tree, inherited), TextStyleBits::DEPENDS_VIEWPORT);
    assert_eq!(bits(&tree, overridden), TextStyleBits::DEPENDS_VIEWPORT);

    // `rem`, `em` and percentages do not depend on the viewport.
    assert!(bits(&tree, root).is_empty());
    assert!(bits(&tree, plain).is_empty());
    assert_eq!(tree.viewport_dependents(), 4);

    // Rebuilding keeps the bits, without flagging anything.
    tree.clear_flags(key);
    build(&mut LayoutInputTreeBuilder::new(), &ui, root, &mut tree).unwrap();
    assert_eq!(bits(&tree, spaced), TextStyleBits::DEPENDS_VIEWPORT);
    assert_eq!(tree.viewport_dependents(), 4);
    assert_clean(&tree, key);
}

#[test]
fn invalidate_viewport_marks_viewport_dependent_items() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let plain = ui.create(Node::Box, ());
    let spaced = ui.create(Node::Box, (bx::LetterSpacing(StyleUnit::Vw(0.01)),));
    let inherited = ui.create(Node::Text, ());
    let independent = ui.create(Node::Text, ());
    let tall = ui.create(Node::Text, (bx::FontSize(StyleUnit::Vh(0.05)),));

    ui.append(root, plain);
    ui.append(plain, spaced);
    ui.append(spaced, inherited);
    ui.append(plain, independent);
    ui.append(root, tall);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    tree.clear_flags(key);

    tree.invalidate_viewport(key);
    assert_eq!(flags2(&tree, spaced), FULL);
    assert_eq!(flags2(&tree, inherited), FULL);
    assert_eq!(flags2(&tree, tall), FULL);
    assert!(flags2(&tree, independent).is_empty());
    assert!(flags2(&tree, plain).is_empty());
    assert_eq!(child_flags(&tree, plain), FULL);
    assert_eq!(child_flags(&tree, root), FULL);

    // An unmarked edit outside the viewport dependent subtrees is not seen.
    ui.set_props(independent, (bx::FontSize(StyleUnit::Px(30.0)),));
    let viewport = Size::new(400.0, 300.0);
    builder.build(&ui, &mut tree, BuildRequest::new(root, 16.0, viewport));

    let style = |node| {
        tree.items[tree.item_for_node(node).unwrap()]
            .text_style
            .clone()
    };
    assert_eq!(style(spaced).span.letter_spacing, 4.0);
    assert_eq!(style(inherited).span.letter_spacing, 4.0);
    assert_eq!(style(tall).span.font_size, 15.0);
    assert_eq!(style(independent).span.font_size, 16.0);
}

#[test]
fn viewport_dependents_are_counted() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let spaced = ui.create(Node::Box, (bx::LetterSpacing(StyleUnit::Vw(0.01)),));
    let inherited = ui.create(Node::Text, ());
    let plain = ui.create(Node::Box, (bx::FontSize(StyleUnit::Rem(2.0)),));

    ui.append(root, spaced);
    ui.append(spaced, inherited);
    ui.append(root, plain);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    let bits = |tree: &LayoutInputTree, node| {
        tree.items[tree.item_for_node(node).unwrap()]
            .text_style
            .bits
    };
    let depends = TextStyleBits::DEPENDS_VIEWPORT;

    // The styled box and the child inheriting from it depend on the viewport.
    assert_eq!(bits(&tree, spaced), depends);
    assert_eq!(bits(&tree, inherited), depends);
    assert!(bits(&tree, root).is_empty());
    assert!(bits(&tree, plain).is_empty());
    assert_eq!(tree.viewport_dependents(), 2);

    // A style that stops depending on the viewport stops its children too.
    tree.clear_flags(key);
    ui.set_props(spaced, (bx::LetterSpacing(StyleUnit::Px(1.0)),));
    tree.mark_node(spaced, FULL);
    build(&mut builder, &ui, root, &mut tree);
    assert!(bits(&tree, spaced).is_empty());
    assert!(bits(&tree, inherited).is_empty());
    assert_eq!(tree.viewport_dependents(), 0);

    // Without dependents, invalidating the viewport marks nothing.
    tree.clear_flags(key);
    tree.invalidate_viewport(key);
    assert_clean(&tree, key);

    // New items are counted.
    let tall = ui.create(Node::Text, (bx::FontSize(StyleUnit::Vh(0.05)),));
    ui.append(plain, tall);
    tree.mark_node(plain, CHILDREN);
    build(&mut builder, &ui, root, &mut tree);
    assert_eq!(bits(&tree, tall), depends);
    assert_eq!(tree.viewport_dependents(), 1);

    // Deleted items are no longer counted.
    tree.clear_flags(key);
    ui.delete(tall);
    tree.mark_node(plain, CHILDREN);
    build(&mut builder, &ui, root, &mut tree);
    assert_eq!(tree.viewport_dependents(), 0);

    tree.clear();
    assert_eq!(tree.viewport_dependents(), 0);
}

#[test]
fn mark_propagates_child_flags_to_ancestors() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let left = ui.create(Node::Box, ());
    let right = ui.create(Node::Box, ());
    let leaf = ui.create(Node::Text, ());

    ui.append(root, left);
    ui.append(root, right);
    ui.append(left, leaf);

    let mut tree = LayoutInputTree::new();
    let key = build(&mut LayoutInputTreeBuilder::new(), &ui, root, &mut tree).unwrap();
    tree.clear_flags(key);

    let leaf_key = tree.item_for_node(leaf).unwrap();
    tree.mark(leaf_key, InputFlags::NEED_BOX_LAYOUT);

    assert_eq!(flags2(&tree, leaf), InputFlags::NEED_BOX_LAYOUT);
    assert!(child_flags(&tree, leaf).is_empty());
    assert!(flags2(&tree, left).is_empty());
    assert_eq!(child_flags(&tree, left), InputFlags::NEED_BOX_LAYOUT);
    assert_eq!(child_flags(&tree, root), InputFlags::NEED_BOX_LAYOUT);
    assert!(child_flags(&tree, right).is_empty());

    // Further flags are added to the ancestors too.
    tree.mark_node(right, CHILDREN);
    assert_eq!(flags2(&tree, right), CHILDREN);
    assert_eq!(
        child_flags(&tree, root),
        InputFlags::NEED_BOX_LAYOUT | CHILDREN
    );
    assert_eq!(child_flags(&tree, left), InputFlags::NEED_BOX_LAYOUT);

    // A full change includes the others.
    tree.mark(leaf_key, FULL);
    assert_eq!(flags2(&tree, leaf), FULL);
    assert_eq!(child_flags(&tree, left), FULL);
    assert!(FULL.contains(InputFlags::NEED_BOX_LAYOUT | CHILDREN));

    // Unmapped nodes are ignored.
    let unmapped = ui.create(Node::Box, ());
    tree.mark_node(unmapped, FULL);
}

#[test]
fn clear_flags_clears_the_subtree() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let left = ui.create(Node::Box, ());
    let right = ui.create(Node::Box, ());
    let leaf = ui.create(Node::Text, ());

    ui.append(root, left);
    ui.append(root, right);
    ui.append(left, leaf);

    let mut tree = LayoutInputTree::new();
    let key = build(&mut LayoutInputTreeBuilder::new(), &ui, root, &mut tree).unwrap();

    // Clearing a subtree keeps the flags of its ancestors and siblings.
    tree.clear_flags(tree.item_for_node(left).unwrap());
    assert!(flags2(&tree, left).is_empty());
    assert!(child_flags(&tree, left).is_empty());
    assert!(flags2(&tree, leaf).is_empty());
    assert_eq!(flags2(&tree, right), FULL);
    assert_eq!(flags2(&tree, root), FULL);

    tree.clear_flags(key);
    assert_clean(&tree, key);
}

#[test]
fn update_reconciles_children_and_keeps_keys() {
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
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    tree.clear_flags(key);

    // Building again without marks changes nothing.
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));
    assert_clean(&tree, key);

    let a_key = tree.item_for_node(a).unwrap();
    let b_key = tree.item_for_node(b).unwrap();
    let inner_key = tree.item_for_node(inner).unwrap();

    // Reorder, delete, and insert.
    let d = ui.create(Node::Text, ());
    ui.before(a, b);
    ui.delete(c);
    ui.append(root, d);
    tree.mark(key, CHILDREN);
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));

    let children: Vec<_> = tree.items.children(key).collect();
    let d_key = tree.item_for_node(d).unwrap();

    assert_eq!(children, vec![b_key, a_key, d_key]);
    assert_eq!(tree.item_for_node(c), None);
    assert_eq!(tree.item_for_node(inner), Some(inner_key));

    // Only the new child is flagged; the flags are left for later steps.
    assert_eq!(tree.items[d_key].flags, FULL);
    assert!(tree.items[a_key].flags.is_empty());
    assert!(tree.items[b_key].flags.is_empty());
    assert!(tree.items[inner_key].flags.is_empty());
    // A child is gone, so the parent is dirty.
    assert_eq!(tree.items[key].flags, FULL);
    assert_eq!(tree.items[key].child_flags, FULL);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn full_layout_resolves_and_marks_the_subtree() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, ());
    let fixed = ui.create(Node::Text, (bx::FontSize(StyleUnit::Px(10.0)),));
    let sibling = ui.create(Node::Box, ());

    ui.append(root, child);
    ui.append(child, text);
    ui.append(child, fixed);
    ui.append(root, sibling);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    tree.clear_flags(key);

    ui.set_props(child, (bx::FontSize(StyleUnit::Px(24.0)),));
    tree.mark_node(child, FULL);
    build(&mut builder, &ui, root, &mut tree).unwrap();

    let item = |node| &tree.items[tree.item_for_node(node).unwrap()];

    assert_eq!(item(child).text_style.span.font_size, 24.0);
    assert_eq!(item(text).text_style.span.font_size, 24.0);
    // Its own font size wins.
    assert_eq!(item(fixed).text_style.span.font_size, 10.0);

    // Every item under the marked one is marked.
    assert_eq!(item(child).flags, FULL);
    assert_eq!(item(text).flags, FULL);
    assert_eq!(item(fixed).flags, FULL);
    assert!(item(root).flags.is_empty());
    assert_eq!(item(root).child_flags, FULL);
    // The unmarked sibling was skipped.
    assert!(item(sibling).flags.is_empty());

    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn full_layout_stops_cascading_where_styles_resolve_as_before() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let outer = ui.create(Node::Box, ());
    let inner = ui.create(Node::Box, (bx::FontSize(StyleUnit::Px(10.0)),));
    let text = ui.create(Node::Text, ());
    let marked = ui.create(Node::Text, ());

    ui.append(root, outer);
    ui.append(outer, inner);
    ui.append(inner, text);
    ui.append(inner, marked);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    tree.clear_flags(key);

    // A display change resolving to the same style and child layout marks
    // no child.
    ui.set_props(outer, (DisplayOuter::Block,));
    tree.mark_node(outer, FULL);
    build(&mut builder, &ui, root, &mut tree).unwrap();

    assert_eq!(flags2(&tree, outer), FULL);
    assert!(flags2(&tree, inner).is_empty());
    assert!(flags2(&tree, text).is_empty());
    assert!(child_flags(&tree, outer).is_empty());
    assert_fresh(&ui, root, &tree, key);
    tree.clear_flags(key);

    // A changed font size cascades until a child sets its own: the inner box
    // resolves as before, so its children keep their flags, but a marked
    // child under it is still resolved.
    ui.set_props(outer, (bx::FontSize(StyleUnit::Px(24.0)),));
    ui.set_props(marked, (bx::FontSize(StyleUnit::Px(12.0)),));
    tree.mark_node(outer, FULL);
    tree.mark_node(marked, FULL);
    build(&mut builder, &ui, root, &mut tree).unwrap();

    let item = |node| &tree.items[tree.item_for_node(node).unwrap()];

    assert_eq!(item(outer).text_style.span.font_size, 24.0);
    assert_eq!(item(inner).flags, FULL);
    assert_eq!(item(inner).text_style.span.font_size, 10.0);
    assert!(item(text).flags.is_empty());
    assert_eq!(item(marked).flags, FULL);
    assert_eq!(item(marked).text_style.span.font_size, 12.0);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn partial_update_of_descendant_uses_parent_style() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, (bx::FontSize(StyleUnit::Px(20.0)),));
    let child = ui.create(Node::Box, ());
    ui.append(root, child);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    tree.clear_flags(key);

    let child_key = tree.item_for_node(child).unwrap();
    ui.set_props(child, (bx::FontSize(StyleUnit::Em(2.0)),));
    tree.mark(child_key, FULL);

    assert_eq!(build(&mut builder, &ui, child, &mut tree), Some(child_key));
    assert_eq!(tree.items[child_key].text_style.span.font_size, 40.0);

    // A new node is inserted by updating the item of its parent.
    tree.clear_flags(key);
    let other_child = ui.create(Node::Text, ());
    ui.append(root, other_child);
    tree.mark_node(root, CHILDREN);
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));

    let inserted = tree.item_for_node(other_child).unwrap();
    assert_eq!(tree.items.parent(inserted), Some(key));
    assert_eq!(tree.items[inserted].flags, FULL);
    assert_eq!(tree.items[inserted].text_style.span.font_size, 20.0);
    assert_eq!(tree.item_for_node(child), Some(child_key));
    assert!(tree.items[child_key].flags.is_empty());

    // A descendant without a parent item means the tree does not mirror the UI.
    let mut other = LayoutInputTree::new();
    assert_eq!(build(&mut builder, &ui, child, &mut other), None);
    assert!(other.items.is_empty());
}

#[test]
fn rebuilding_a_node_updates_its_kind_and_deleting_marks_its_parent() {
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
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    let paragraph_key = tree.item_for_node(paragraph).unwrap();
    let span_key = tree.item_for_node(span).unwrap();
    let interruption_key = tree.item_for_node(interruption).unwrap();

    // Child layouts come from each node's own display, not from its content.
    assert_eq!(tree.items[paragraph_key].child_layout, ChildLayout::Block);
    assert_eq!(tree.items[span_key].child_layout, ChildLayout::Inline);
    tree.clear_flags(key);

    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    tree.mark_node(interruption, FULL);
    assert_eq!(
        build(&mut builder, &ui, interruption, &mut tree),
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
    assert_fresh(&ui, root, &tree, key);

    // Deleting a node marks its parent dirty.
    tree.clear_flags(key);
    ui.delete(interruption);
    assert_eq!(build(&mut builder, &ui, interruption, &mut tree), None);
    assert_eq!(tree.item_for_node(interruption), None);
    assert_eq!(tree.items[span_key].flags, FULL);
    assert_eq!(tree.items[paragraph_key].child_flags, FULL);
    assert_eq!(tree.items[key].child_flags, FULL);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn build_resolves_only_marked_items_of_an_existing_tree() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Box, (bx::FontSize(StyleUnit::Rem(2.0)),));
    let text = ui.create(Node::Text, (bx::LetterSpacing(StyleUnit::Vw(0.01)),));

    ui.append(root, child);
    ui.append(child, text);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    let child_key = tree.item_for_node(child).unwrap();
    let text_key = tree.item_for_node(text).unwrap();
    tree.clear_flags(key);

    // Unmarked items keep the units they were resolved with.
    let viewport = Size::new(1000.0, 600.0);
    assert_eq!(
        builder.build(&ui, &mut tree, BuildRequest::new(root, 20.0, viewport)),
        Some(key)
    );
    assert_eq!(tree.items[child_key].text_style.span.font_size, 32.0);
    assert_eq!(tree.items[text_key].text_style.span.letter_spacing, 8.0);
    assert_clean(&tree, key);

    // Marked items resolve with the request.
    tree.mark(text_key, FULL);
    builder.build(&ui, &mut tree, BuildRequest::new(root, 16.0, viewport));
    assert_eq!(tree.items[text_key].text_style.span.letter_spacing, 10.0);
    assert_eq!(tree.items[text_key].flags, FULL);
    assert_eq!(tree.items[child_key].child_flags, FULL);

    // Flags the builder does not consume are kept.
    tree.clear_flags(key);
    tree.mark(text_key, InputFlags::NEED_BOX_LAYOUT);
    builder.build(&ui, &mut tree, BuildRequest::new(root, 16.0, viewport));
    assert_eq!(tree.items[text_key].flags, InputFlags::NEED_BOX_LAYOUT);
}

#[test]
fn build_builds_a_new_tree_without_a_source_item() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Box, (bx::FontSize(StyleUnit::Rem(2.0)),));
    ui.append(root, child);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();

    // A new root font size needs a new tree.
    tree.delete_item(key);
    let rebuilt = builder
        .build(&ui, &mut tree, BuildRequest::new(root, 20.0, VIEWPORT))
        .unwrap();

    assert_eq!(tree.items.descendants(rebuilt).count(), 2);
    for item in tree.items.descendants(rebuilt) {
        assert_eq!(tree.items[item].flags, FULL);
    }
    let child_key = tree.item_for_node(child).unwrap();
    assert_eq!(tree.items[child_key].text_style.span.font_size, 40.0);
}

#[test]
fn update_reconciles_marked_child_lists() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let a = ui.create(Node::Box, ());
    let b = ui.create(Node::Box, ());
    let inner = ui.create(Node::Box, ());

    ui.append(root, a);
    ui.append(root, b);
    ui.append(a, inner);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    let a_key = tree.item_for_node(a).unwrap();
    let b_key = tree.item_for_node(b).unwrap();
    tree.clear_flags(key);

    // Insert under a nested item, and reorder and insert under the root.
    let text = ui.create(Node::Text, ());
    let nested = ui.create(Node::Text, ());
    ui.append(root, text);
    ui.before(a, b);
    ui.append(inner, nested);
    tree.mark(key, CHILDREN);
    tree.mark_node(inner, CHILDREN);
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));

    let text_key = tree.item_for_node(text).unwrap();
    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        vec![b_key, a_key, text_key]
    );
    assert_eq!(tree.items[text_key].flags, FULL);
    assert_eq!(flags2(&tree, nested), FULL);
    assert!(tree.items[a_key].flags.is_empty());
    assert_eq!(tree.items[a_key].child_flags, FULL);
    assert!(tree.items[b_key].flags.is_empty());
    assert_fresh(&ui, root, &tree, key);

    // Deleting a node marks its parent.
    tree.clear_flags(key);
    ui.delete(a);
    tree.mark(key, CHILDREN);
    build(&mut builder, &ui, root, &mut tree);
    assert_eq!(tree.item_for_node(a), None);
    assert_eq!(tree.item_for_node(inner), None);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn update_reconciles_children_when_child_layout_changes() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let parent = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, ());
    ui.append(root, parent);
    ui.append(parent, text);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    tree.clear_flags(key);

    // Content display has no child items.
    ui.set_props(parent, (DisplayInner::Content,));
    tree.mark_node(parent, FULL);
    build(&mut builder, &ui, root, &mut tree);
    assert_eq!(tree.item_for_node(text), None);
    assert_fresh(&ui, root, &tree, key);

    // Back to flow, the child is built again.
    tree.clear_flags(key);
    ui.remove_prop::<DisplayInner>(parent);
    tree.mark_node(parent, FULL);
    build(&mut builder, &ui, root, &mut tree);
    let text_key = tree.item_for_node(text).unwrap();
    assert_eq!(tree.items[text_key].flags, FULL);
    assert_fresh(&ui, root, &tree, key);
}

#[test]
fn deleting_and_moving_children_keeps_the_others_unflagged() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let nodes: Vec<_> = (0..4)
        .map(|_| {
            let node = ui.create(Node::Box, ());
            ui.append(root, node);
            node
        })
        .collect();

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    let keys: Vec<_> = nodes
        .iter()
        .map(|&node| tree.item_for_node(node).unwrap())
        .collect();
    tree.clear_flags(key);

    ui.delete(nodes[0]);
    tree.mark(key, CHILDREN);
    build(&mut builder, &ui, root, &mut tree);

    assert_eq!(tree.items.children(key).collect::<Vec<_>>(), keys[1..]);
    for &item in &keys[1..] {
        assert!(tree.items[item].flags.is_empty());
    }
    assert_eq!(tree.items[key].flags, FULL);

    tree.clear_flags(key);
    ui.prepend(root, nodes[3]);
    tree.mark(key, CHILDREN);
    build(&mut builder, &ui, root, &mut tree);

    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        vec![keys[3], keys[1], keys[2]]
    );
    for &item in &keys[1..] {
        assert!(tree.items[item].flags.is_empty());
    }
}
