use std::sync::Arc;

use comet_layout::{
    input::{
        ChildLayout, LayoutFlags, LayoutInputKey, LayoutInputKind,
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
    let bits = |node| {
        tree.items[tree.item_for_node(node).unwrap()]
            .text_style
            .bits
    };

    assert_eq!(bits(spaced), TextStyleBits::DEPENDS_VIEWPORT);
    assert_eq!(bits(tall), TextStyleBits::DEPENDS_VIEWPORT);
    // Descendants inherit values resolved with the viewport.
    assert_eq!(bits(inherited), TextStyleBits::DEPENDS_VIEWPORT);
    assert_eq!(bits(overridden), TextStyleBits::DEPENDS_VIEWPORT);

    // `rem`, `em` and percentages do not depend on the viewport.
    assert!(bits(root).is_empty());
    assert!(bits(plain).is_empty());
}

#[test]
fn update_viewport_builds_only_viewport_dependent_subtrees() {
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
    assert_eq!(
        builder.update_viewport(&ui, &mut tree, BuildRequest::new(root, 16.0, viewport)),
        Some(key)
    );

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
    builder.update_viewport(&ui, &mut tree, BuildRequest::new(root, 16.0, viewport));
    for item in tree.items.descendants(key) {
        assert!(tree.items[item].flags.is_empty());
    }

    // Nodes without items are ignored.
    let unmapped = ui.create(Node::Box, ());
    assert_eq!(
        builder.update_viewport(&ui, &mut tree, BuildRequest::new(unmapped, 16.0, viewport)),
        None
    );
}

#[test]
fn tree_counts_viewport_dependent_items() {
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

    // The styled box and the child inheriting from it.
    assert_eq!(tree.viewport_dependent_count(), 2);

    // A style that stops depending on the viewport is no longer counted.
    ui.set_props(spaced, (bx::LetterSpacing(StyleUnit::Px(1.0)),));
    build(&mut builder, &ui, root, &mut tree);
    assert_eq!(tree.viewport_dependent_count(), 0);

    // With nothing to update, update_viewport visits and flags nothing.
    clean(&mut tree, key);
    assert_eq!(
        builder.update_viewport(
            &ui,
            &mut tree,
            BuildRequest::new(root, 16.0, Size::new(400.0, 300.0))
        ),
        Some(key)
    );
    for item in tree.items.descendants(key) {
        assert!(tree.items[item].flags.is_empty());
    }

    // New and deleted items update the count.
    let tall = ui.create(Node::Text, (bx::FontSize(StyleUnit::Vh(0.05)),));
    ui.append(plain, tall);
    build(&mut builder, &ui, root, &mut tree);
    assert_eq!(tree.viewport_dependent_count(), 1);

    ui.delete(plain);
    build(&mut builder, &ui, root, &mut tree);
    assert_eq!(tree.viewport_dependent_count(), 0);

    tree.clear();
    assert_eq!(tree.viewport_dependent_count(), 0);
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

    // Every build re-resolves the subtree, but nothing is flagged when
    // nothing changed.
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
    assert_eq!(build(&mut builder, &ui, root, &mut tree), Some(key));

    let children: Vec<_> = tree.items.children(key).collect();
    let d_key = tree.item_for_node(d).unwrap();

    assert_eq!(children, vec![b_key, a_key, d_key]);
    assert_eq!(tree.item_for_node(c), None);
    assert_eq!(tree.item_for_node(inner), Some(inner_key));
    assert_eq!(tree.items[d_key].flags, LayoutFlags::all());

    // Unchanged children are not flagged; the reordered parent is fully dirty.
    assert_eq!(tree.items[a_key].flags, LayoutFlags::empty());
    assert_eq!(tree.items[b_key].flags, LayoutFlags::empty());
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
    tree.mark(key, LayoutFlags::NEED_FULL_LAYOUT);
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
    tree.mark(child_key, LayoutFlags::NEED_FULL_LAYOUT);

    assert_eq!(build(&mut builder, &ui, child, &mut tree), Some(child_key));
    assert_eq!(tree.items[child_key].text_style.span.font_size, 40.0);

    // A new node is inserted by updating the item of its parent.
    clean(&mut tree, key);
    let other_child = ui.create(Node::Text, ());
    ui.append(root, other_child);
    tree.mark_node(root, LayoutFlags::NEED_FULL_LAYOUT);
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
    tree.mark_node(interruption, LayoutFlags::NEED_FULL_LAYOUT);
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
    assert_eq!(
        tree.items[paragraph_key].flags,
        LayoutFlags::CHILD_NEED_LAYOUT
    );
    assert_eq!(tree.items[span_key].flags, LayoutFlags::CHILD_NEED_LAYOUT);
    assert_fresh(&ui, root, &tree, key);

    // Deleting a node flags its parent's child list.
    clean(&mut tree, key);
    ui.delete(interruption);
    assert_eq!(build(&mut builder, &ui, interruption, &mut tree), None);
    assert_eq!(tree.item_for_node(interruption), None);
    assert_eq!(tree.items[span_key].flags, LayoutFlags::NEED_FULL_LAYOUT);
    assert_eq!(
        tree.items[paragraph_key].flags,
        LayoutFlags::CHILD_NEED_LAYOUT
    );
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
fn build_re_resolves_and_flags_clean_subtrees() {
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

    // A clean tree picks up the new root font size and viewport. Keys are kept.
    let viewport = Size::new(1000.0, 600.0);
    assert_eq!(
        builder.build(&ui, &mut tree, BuildRequest::new(root, 20.0, viewport)),
        Some(key)
    );
    assert_eq!(tree.item_for_node(child), Some(child_key));
    assert_eq!(tree.item_for_node(text), Some(text_key));
    assert_eq!(tree.items[key].text_style.span.font_size, 20.0);
    assert_eq!(tree.items[child_key].text_style.span.font_size, 40.0);
    assert_eq!(tree.items[text_key].text_style.span.letter_spacing, 10.0);

    assert_eq!(
        tree.items[key].flags,
        LayoutFlags::NEED_FULL_LAYOUT | LayoutFlags::CHILD_NEED_LAYOUT
    );
    assert_eq!(
        tree.items[child_key].flags,
        LayoutFlags::NEED_FULL_LAYOUT | LayoutFlags::CHILD_NEED_LAYOUT
    );
    assert_eq!(tree.items[text_key].flags, LayoutFlags::NEED_FULL_LAYOUT);

    // Building again with the same root font size and viewport resolves equal
    // items, so nothing is flagged.
    clean(&mut tree, key);
    builder.build(&ui, &mut tree, BuildRequest::new(root, 20.0, viewport));
    for item in tree.items.descendants(key) {
        assert_eq!(tree.items[item].flags, LayoutFlags::empty());
    }

    // A new viewport flags only the items whose text style resolves
    // differently.
    clean(&mut tree, key);
    let wider = Size::new(500.0, 600.0);
    builder.build(&ui, &mut tree, BuildRequest::new(root, 20.0, wider));

    assert_eq!(tree.items[key].flags, LayoutFlags::CHILD_NEED_LAYOUT);
    assert_eq!(tree.items[child_key].flags, LayoutFlags::CHILD_NEED_LAYOUT);
    assert_eq!(tree.items[text_key].flags, LayoutFlags::NEED_FULL_LAYOUT);
    assert_eq!(tree.items[text_key].text_style.span.letter_spacing, 5.0);

    // Marked content changes are kept.
    clean(&mut tree, key);
    tree.mark(text_key, LayoutFlags::NEED_FULL_LAYOUT);
    builder.build(&ui, &mut tree, BuildRequest::new(root, 20.0, wider));
    assert_eq!(tree.items[text_key].flags, LayoutFlags::NEED_FULL_LAYOUT);
}
