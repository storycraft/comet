use std::sync::Arc;

use comet_layout::{
    input::{
        ChildLayout, InputFlags, LayoutFlags, LayoutInputKey, LayoutInputKind,
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

fn build(
    builder: &mut LayoutInputTreeBuilder,
    ui: &Ui,
    root: comet_tree::node::NodeKey,
    tree: &mut LayoutInputTree,
) -> Option<LayoutInputKey> {
    builder.build(ui, tree, BuildRequest::new(root, 16.0, VIEWPORT))
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
                item.text_style.span.font_size,
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
    build(&mut LayoutInputTreeBuilder::new(), &ui, root, &mut tree).unwrap();
    let bits = |node| tree.items[tree.item_for_node(node).unwrap()].bits;
    let child = TextStyleBits::CHILD_DEPENDS_VIEWPORT;

    // Viewport dependent descendants mark their ancestors up to the root.
    assert_eq!(bits(spaced), TextStyleBits::DEPENDS_VIEWPORT | child);
    assert_eq!(bits(tall), TextStyleBits::DEPENDS_VIEWPORT);
    // Descendants inherit values resolved with the viewport.
    assert_eq!(bits(inherited), TextStyleBits::DEPENDS_VIEWPORT);
    assert_eq!(bits(overridden), TextStyleBits::DEPENDS_VIEWPORT);

    // `rem`, `em` and percentages do not depend on the viewport.
    assert_eq!(bits(root), child);
    assert!(bits(plain).is_empty());

    // Rebuilding keeps the marks, without flagging anything.
    let key = tree.item_for_node(root).unwrap();
    clean(&mut tree, key);
    build(&mut LayoutInputTreeBuilder::new(), &ui, root, &mut tree).unwrap();
    assert_eq!(tree.items[key].bits, child);
    for item in tree.items.descendants(key) {
        assert!(tree.items[item].flags.is_empty());
    }
}

#[test]
fn viewport_change_builds_only_viewport_dependent_subtrees() {
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
    clean(&mut tree, key);

    let item = |tree: &LayoutInputTree, node| tree.item_for_node(node).unwrap();
    assert_eq!(
        tree.items[item(&tree, spaced)]
            .text_style
            .span
            .letter_spacing,
        8.0
    );
    assert_eq!(
        tree.items[item(&tree, tall)].text_style.span.font_size,
        30.0
    );

    // An unmarked edit outside the viewport dependent subtrees is not seen.
    ui.set_props(independent, (bx::FontSize(StyleUnit::Px(30.0)),));

    let viewport = Size::new(400.0, 300.0);
    let changed = BuildRequest::new(root, 16.0, viewport).with_viewport_changed(true);
    assert_eq!(builder.build(&ui, &mut tree, changed), Some(key));

    let style = |node| tree.items[item(&tree, node)].text_style.clone();
    let flags = |node| tree.items[item(&tree, node)].flags;

    assert_eq!(style(spaced).span.letter_spacing, 4.0);
    assert_eq!(style(inherited).span.letter_spacing, 4.0);
    assert_eq!(style(tall).span.font_size, 15.0);
    assert_eq!(style(independent).span.font_size, 16.0);

    assert_eq!(
        flags(spaced),
        LayoutFlags::NEED_FULL_LAYOUT | LayoutFlags::CHILD_NEED_LAYOUT
    );
    assert_eq!(flags(inherited), LayoutFlags::NEED_FULL_LAYOUT);
    assert_eq!(flags(tall), LayoutFlags::NEED_FULL_LAYOUT);
    assert!(flags(independent).is_empty());
    assert_eq!(flags(plain), LayoutFlags::CHILD_NEED_LAYOUT);
    assert_eq!(flags(root), LayoutFlags::CHILD_NEED_LAYOUT);

    // The same viewport flags nothing.
    clean(&mut tree, key);
    builder.build(&ui, &mut tree, changed);
    for item in tree.items.descendants(key) {
        assert!(tree.items[item].flags.is_empty());
    }
    assert_no_input_flags(&tree, key);

    // Marked items are resolved in the same build as the viewport change.
    tree.mark_input_node(independent, InputFlags::NEED_STYLE);
    let wider = Size::new(800.0, 300.0);
    builder.build(
        &ui,
        &mut tree,
        BuildRequest::new(root, 16.0, wider).with_viewport_changed(true),
    );
    let style = |node| tree.items[item(&tree, node)].text_style.clone();
    assert_eq!(style(spaced).span.letter_spacing, 8.0);
    assert_eq!(style(independent).span.font_size, 30.0);
    assert_no_input_flags(&tree, key);
}

#[test]
fn viewport_dependent_items_mark_their_ancestors() {
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
    let bits = |tree: &LayoutInputTree, node| tree.items[tree.item_for_node(node).unwrap()].bits;
    let depends = TextStyleBits::DEPENDS_VIEWPORT;
    let child = TextStyleBits::CHILD_DEPENDS_VIEWPORT;

    // The styled box and the child inheriting from it depend on the
    // viewport; their ancestors are marked.
    assert_eq!(bits(&tree, spaced), depends | child);
    assert_eq!(bits(&tree, inherited), depends);
    assert_eq!(bits(&tree, root), child);
    assert!(bits(&tree, plain).is_empty());

    // A style that stops depending on the viewport keeps the marks of its
    // ancestors, which are only added.
    ui.set_props(spaced, (bx::LetterSpacing(StyleUnit::Px(1.0)),));
    tree.mark_input_node(spaced, InputFlags::NEED_STYLE);
    build(&mut builder, &ui, root, &mut tree);
    assert_eq!(bits(&tree, spaced), child);
    assert!(bits(&tree, inherited).is_empty());
    assert_eq!(bits(&tree, root), child);

    // Stale marks only lead a viewport change to items it leaves as they are.
    clean(&mut tree, key);
    assert_eq!(
        builder.build(
            &ui,
            &mut tree,
            BuildRequest::new(root, 16.0, Size::new(400.0, 300.0)).with_viewport_changed(true)
        ),
        Some(key)
    );
    for item in tree.items.descendants(key) {
        assert!(tree.items[item].flags.is_empty());
    }

    // New items mark their ancestors.
    let tall = ui.create(Node::Text, (bx::FontSize(StyleUnit::Vh(0.05)),));
    ui.append(plain, tall);
    tree.mark_input_node(plain, InputFlags::NEED_CHILDREN);
    build(&mut builder, &ui, root, &mut tree);
    assert_eq!(bits(&tree, tall), depends);
    assert_eq!(bits(&tree, plain), child);
}

#[test]
fn mark_propagates_to_ancestors() {
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
    clean(&mut tree, key);

    let leaf_key = tree.item_for_node(leaf).unwrap();
    tree.mark(leaf_key, LayoutFlags::NEED_FULL_LAYOUT);

    assert_eq!(tree.items[leaf_key].flags, LayoutFlags::NEED_FULL_LAYOUT);
    assert_eq!(
        tree.items[tree.item_for_node(left).unwrap()].flags,
        LayoutFlags::CHILD_NEED_LAYOUT
    );
    assert_eq!(tree.items[key].flags, LayoutFlags::CHILD_NEED_LAYOUT);
    assert!(
        tree.items[tree.item_for_node(right).unwrap()]
            .flags
            .is_empty()
    );

    tree.mark_node(right, LayoutFlags::NEED_FULL_LAYOUT);
    assert_eq!(
        tree.items[tree.item_for_node(right).unwrap()].flags,
        LayoutFlags::NEED_FULL_LAYOUT
    );

    // Box style changes flag only the box, and propagate the same way.
    clean(&mut tree, key);
    tree.mark_node(leaf, LayoutFlags::NEED_BOX_LAYOUT);

    assert_eq!(tree.items[leaf_key].flags, LayoutFlags::NEED_BOX_LAYOUT);
    assert_eq!(
        tree.items[tree.item_for_node(left).unwrap()].flags,
        LayoutFlags::CHILD_NEED_LAYOUT
    );
    assert_eq!(tree.items[key].flags, LayoutFlags::CHILD_NEED_LAYOUT);

    // A full change includes the box change.
    tree.mark(leaf_key, LayoutFlags::NEED_FULL_LAYOUT);
    assert_eq!(tree.items[leaf_key].flags, LayoutFlags::NEED_FULL_LAYOUT);
    assert!(LayoutFlags::NEED_FULL_LAYOUT.contains(LayoutFlags::NEED_BOX_LAYOUT));

    // Unmapped nodes are ignored.
    let unmapped = ui.create(Node::Box, ());
    tree.mark_node(unmapped, LayoutFlags::NEED_FULL_LAYOUT);
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
    clean(&mut tree, key);

    // Building again without marks changes nothing.
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));
    for item in tree.items.descendants(key) {
        assert_eq!(tree.items[item].flags, LayoutFlags::empty());
    }

    clean(&mut tree, key);

    let a_key = tree.item_for_node(a).unwrap();
    let b_key = tree.item_for_node(b).unwrap();
    let inner_key = tree.item_for_node(inner).unwrap();

    // Reorder, delete, and insert.
    let d = ui.create(Node::Text, ());
    ui.before(a, b);
    ui.delete(c);
    ui.append(root, d);
    tree.mark_input(key, InputFlags::NEED_CHILDREN);
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));

    let children: Vec<_> = tree.items.children(key).collect();
    let d_key = tree.item_for_node(d).unwrap();

    assert_eq!(children, vec![b_key, a_key, d_key]);
    assert_eq!(tree.item_for_node(c), None);
    assert_eq!(tree.item_for_node(inner), Some(inner_key));
    assert_eq!(tree.items[d_key].flags, LayoutFlags::all());

    // Unchanged children are not flagged, the moved child is only marked
    // moved, and the reordered parent is fully dirty.
    assert_eq!(tree.items[a_key].flags, LayoutFlags::empty());
    assert_eq!(tree.items[b_key].flags, LayoutFlags::MOVED);
    assert_eq!(tree.items[inner_key].flags, LayoutFlags::empty());
    assert_eq!(
        tree.items[key].flags,
        LayoutFlags::NEED_FULL_LAYOUT | LayoutFlags::CHILD_NEED_LAYOUT
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
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    clean(&mut tree, key);

    ui.set_props(root, (bx::FontSize(StyleUnit::Px(24.0)),));
    tree.mark_input(key, InputFlags::NEED_STYLE);
    build(&mut builder, &ui, root, &mut tree).unwrap();

    let item = |node| &tree.items[tree.item_for_node(node).unwrap()];

    assert_eq!(item(text).text_style.span.font_size, 24.0);
    assert_eq!(item(text).flags, LayoutFlags::NEED_FULL_LAYOUT);
    assert_eq!(
        item(child).flags,
        LayoutFlags::NEED_FULL_LAYOUT | LayoutFlags::CHILD_NEED_LAYOUT
    );
    // Its own font size wins, so the resolved style is unchanged and it is
    // not flagged.
    assert_eq!(item(fixed).text_style.span.font_size, 10.0);
    assert_eq!(item(fixed).flags, LayoutFlags::empty());

    let mut fresh = LayoutInputTree::new();
    let fresh_key = build(&mut builder, &ui, root, &mut fresh).unwrap();
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
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    clean(&mut tree, key);

    let child_key = tree.item_for_node(child).unwrap();
    ui.set_props(child, (bx::FontSize(StyleUnit::Em(2.0)),));
    tree.mark_input(child_key, InputFlags::NEED_STYLE);

    assert_eq!(build(&mut builder, &ui, child, &mut tree), Some(child_key));
    assert_eq!(tree.items[child_key].text_style.span.font_size, 40.0);

    // A new node is inserted by updating the item of its parent.
    clean(&mut tree, key);
    let other_child = ui.create(Node::Text, ());
    ui.append(root, other_child);
    tree.mark_input_node(root, InputFlags::NEED_CHILDREN);
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));

    let inserted = tree.item_for_node(other_child).unwrap();
    assert_eq!(tree.items.parent(inserted), Some(key));
    assert_eq!(tree.items[inserted].flags, LayoutFlags::all());
    assert_eq!(tree.item_for_node(child), Some(child_key));
    assert_eq!(tree.items[child_key].flags, LayoutFlags::empty());

    // A descendant without a parent item means the tree does not mirror the UI.
    let mut other = LayoutInputTree::new();
    assert_eq!(build(&mut builder, &ui, child, &mut other), None);
    assert!(other.items.is_empty());
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
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    let paragraph_key = tree.item_for_node(paragraph).unwrap();
    let span_key = tree.item_for_node(span).unwrap();
    let interruption_key = tree.item_for_node(interruption).unwrap();

    // Child layouts come from each node's own display, not from its content.
    assert_eq!(tree.items[paragraph_key].child_layout, ChildLayout::Block);
    assert_eq!(tree.items[span_key].child_layout, ChildLayout::Inline);
    clean(&mut tree, key);

    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    tree.mark_input_node(interruption, InputFlags::NEED_STYLE);
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
    assert_eq!(tree.items[paragraph_key].child_layout, ChildLayout::Block);
    // The paragraph owns the span's inline content, so it lays out the
    // interruption by its kind and is marked. The span is not.
    assert_eq!(
        tree.items[paragraph_key].flags,
        LayoutFlags::NEED_FULL_LAYOUT | LayoutFlags::CHILD_NEED_LAYOUT
    );
    assert_eq!(tree.items[span_key].flags, LayoutFlags::CHILD_NEED_LAYOUT);
    assert_fresh(&ui, root, &tree, key);

    // Deleting a node flags its parent's child list, and the owner of the
    // inline element's content.
    clean(&mut tree, key);
    ui.delete(interruption);
    assert_eq!(build(&mut builder, &ui, interruption, &mut tree), None);
    assert_eq!(tree.item_for_node(interruption), None);
    assert_eq!(tree.items[span_key].flags, LayoutFlags::NEED_FULL_LAYOUT);
    assert_eq!(
        tree.items[paragraph_key].flags,
        LayoutFlags::NEED_FULL_LAYOUT | LayoutFlags::CHILD_NEED_LAYOUT
    );
    // Block children only mark their parent.
    assert_eq!(tree.items[key].flags, LayoutFlags::CHILD_NEED_LAYOUT);
    assert_fresh(&ui, root, &tree, key);
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
    clean(&mut tree, key);

    // Unmarked items keep the units they were resolved with.
    let viewport = Size::new(1000.0, 600.0);
    assert_eq!(
        builder.build(&ui, &mut tree, BuildRequest::new(root, 20.0, viewport)),
        Some(key)
    );
    assert_eq!(tree.items[child_key].text_style.span.font_size, 32.0);
    assert_eq!(tree.items[text_key].text_style.span.letter_spacing, 8.0);
    for item in tree.items.descendants(key) {
        assert_eq!(tree.items[item].flags, LayoutFlags::empty());
    }

    // Marked items resolve with the request.
    tree.mark_input(text_key, InputFlags::NEED_STYLE);
    builder.build(&ui, &mut tree, BuildRequest::new(root, 16.0, viewport));
    assert_eq!(tree.items[text_key].text_style.span.letter_spacing, 10.0);
    assert_eq!(tree.items[text_key].flags, LayoutFlags::NEED_FULL_LAYOUT);
    assert_eq!(tree.items[child_key].flags, LayoutFlags::CHILD_NEED_LAYOUT);

    // Marked content changes are kept.
    clean(&mut tree, key);
    tree.mark(text_key, LayoutFlags::NEED_FULL_LAYOUT);
    builder.build(&ui, &mut tree, BuildRequest::new(root, 16.0, viewport));
    assert_eq!(tree.items[text_key].flags, LayoutFlags::NEED_FULL_LAYOUT);
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
    clean(&mut tree, key);

    // A new root font size needs a new tree.
    tree.delete_item(key);
    let rebuilt = builder
        .build(&ui, &mut tree, BuildRequest::new(root, 20.0, VIEWPORT))
        .unwrap();

    assert_eq!(tree.items.descendants(rebuilt).count(), 2);
    for item in tree.items.descendants(rebuilt) {
        assert_eq!(tree.items[item].flags, LayoutFlags::all());
    }
    let child_key = tree.item_for_node(child).unwrap();
    assert_eq!(tree.items[child_key].text_style.span.font_size, 40.0);
}

fn assert_no_input_flags(tree: &LayoutInputTree, root: LayoutInputKey) {
    for key in tree.items.descendants(root) {
        assert_eq!(tree.items[key].input_flags, InputFlags::empty());
    }
}

#[test]
fn mark_input_propagates_to_ancestors() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, ());
    ui.append(root, child);
    ui.append(child, text);

    let mut tree = LayoutInputTree::new();
    let key = build(&mut LayoutInputTreeBuilder::new(), &ui, root, &mut tree).unwrap();
    assert_no_input_flags(&tree, key);

    tree.mark_input_node(text, InputFlags::NEED_STYLE);

    let item = |node| tree.items[tree.item_for_node(node).unwrap()].input_flags;
    assert_eq!(item(text), InputFlags::NEED_STYLE);
    assert_eq!(item(child), InputFlags::CHILD_NEED_INPUT);
    assert_eq!(item(root), InputFlags::CHILD_NEED_INPUT);
}

#[test]
fn update_resolves_only_marked_items() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let marked = ui.create(Node::Box, ());
    let inherited = ui.create(Node::Text, ());
    let unmarked = ui.create(Node::Text, ());

    ui.append(root, marked);
    ui.append(marked, inherited);
    ui.append(root, unmarked);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    clean(&mut tree, key);

    let item = |tree: &LayoutInputTree, node| tree.items[tree.item_for_node(node).unwrap()].clone();

    // A clean tree is not visited.
    ui.set_props(unmarked, (bx::FontSize(StyleUnit::Px(30.0)),));
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));
    assert_eq!(item(&tree, unmarked).text_style.span.font_size, 16.0);

    // A marked item is resolved, with its descendants inheriting the change.
    ui.set_props(marked, (bx::FontSize(StyleUnit::Px(24.0)),));
    tree.mark_input_node(marked, InputFlags::NEED_STYLE);
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));

    assert_eq!(item(&tree, marked).text_style.span.font_size, 24.0);
    assert_eq!(item(&tree, inherited).text_style.span.font_size, 24.0);
    assert_eq!(
        item(&tree, marked).flags,
        LayoutFlags::NEED_FULL_LAYOUT | LayoutFlags::CHILD_NEED_LAYOUT
    );
    assert_eq!(item(&tree, inherited).flags, LayoutFlags::NEED_FULL_LAYOUT);
    assert_eq!(item(&tree, root).flags, LayoutFlags::CHILD_NEED_LAYOUT);
    // The unmarked sibling was skipped.
    assert_eq!(item(&tree, unmarked).text_style.span.font_size, 16.0);
    assert_eq!(item(&tree, unmarked).flags, LayoutFlags::empty());
    assert_no_input_flags(&tree, key);
}

#[test]
fn update_stops_where_text_styles_stop_changing() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Box, ());
    ui.append(root, child);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();
    clean(&mut tree, key);

    // The root resolves to an equal style, so its unmarked child is kept.
    ui.set_props(child, (bx::FontSize(StyleUnit::Px(30.0)),));
    tree.mark_input(key, InputFlags::NEED_STYLE);
    build(&mut builder, &ui, root, &mut tree);

    let child_key = tree.item_for_node(child).unwrap();
    assert_eq!(tree.items[child_key].text_style.span.font_size, 16.0);
    assert_eq!(tree.items[key].flags, LayoutFlags::empty());
    assert_no_input_flags(&tree, key);
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
    clean(&mut tree, key);

    // Insert under a nested item, and reorder and insert under the root.
    let text = ui.create(Node::Text, ());
    let nested = ui.create(Node::Text, ());
    ui.append(root, text);
    ui.before(a, b);
    ui.append(inner, nested);
    tree.mark_input(key, InputFlags::NEED_CHILDREN);
    tree.mark_input_node(inner, InputFlags::NEED_CHILDREN);
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));

    let text_key = tree.item_for_node(text).unwrap();
    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        vec![b_key, a_key, text_key]
    );
    assert_eq!(tree.items[text_key].flags, LayoutFlags::all());
    assert!(tree.item_for_node(nested).is_some());
    assert_eq!(tree.items[a_key].flags, LayoutFlags::CHILD_NEED_LAYOUT);
    assert_eq!(tree.items[b_key].flags, LayoutFlags::MOVED);
    assert_no_input_flags(&tree, key);
    assert_fresh(&ui, root, &tree, key);

    // Deleting a node marks its parent.
    clean(&mut tree, key);
    ui.delete(a);
    tree.mark_input(key, InputFlags::NEED_CHILDREN);
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
    clean(&mut tree, key);

    // Content display has no child items.
    ui.set_props(parent, (DisplayInner::Content,));
    tree.mark_input_node(parent, InputFlags::NEED_STYLE);
    build(&mut builder, &ui, root, &mut tree);
    assert_eq!(tree.item_for_node(text), None);
    assert_fresh(&ui, root, &tree, key);

    // Back to flow, the child is built again.
    ui.remove_prop::<DisplayInner>(parent);
    tree.mark_input_node(parent, InputFlags::NEED_STYLE);
    build(&mut builder, &ui, root, &mut tree);
    let text_key = tree.item_for_node(text).unwrap();
    assert_eq!(tree.items[text_key].flags, LayoutFlags::all());
    assert_fresh(&ui, root, &tree, key);
    assert_no_input_flags(&tree, key);
}

#[test]
fn build_clears_input_flags() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Box, ());
    ui.append(root, child);

    let mut tree = LayoutInputTree::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree).unwrap();

    tree.mark_input_node(child, InputFlags::NEED_STYLE | InputFlags::NEED_CHILDREN);
    build(&mut builder, &ui, root, &mut tree);
    assert_no_input_flags(&tree, key);
}

#[test]
fn only_moved_children_are_marked_moved() {
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
    clean(&mut tree, key);

    // Deleting the first child moves nothing.
    ui.delete(nodes[0]);
    tree.mark_input(key, InputFlags::NEED_CHILDREN);
    build(&mut builder, &ui, root, &mut tree);

    assert_eq!(tree.items.children(key).collect::<Vec<_>>(), keys[1..]);
    for &item in &keys[1..] {
        assert_eq!(tree.items[item].flags, LayoutFlags::empty());
    }
    assert_eq!(tree.items[key].flags, LayoutFlags::NEED_FULL_LAYOUT);

    // Moving the last child to the front marks only it.
    clean(&mut tree, key);
    ui.prepend(root, nodes[3]);
    tree.mark_input(key, InputFlags::NEED_CHILDREN);
    build(&mut builder, &ui, root, &mut tree);

    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        vec![keys[3], keys[1], keys[2]]
    );
    assert_eq!(tree.items[keys[3]].flags, LayoutFlags::MOVED);
    assert_eq!(tree.items[keys[1]].flags, LayoutFlags::empty());
    assert_eq!(tree.items[keys[2]].flags, LayoutFlags::empty());
}
