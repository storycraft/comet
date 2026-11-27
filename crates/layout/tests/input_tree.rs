use std::num::NonZeroU64;

use comet_content::ContentId;
use std::sync::Arc;

use comet_geometry::{Size, StyleUnit};
use comet_layout::{
    dirty_map::{DirtyFlags, DirtyMap},
    input::{
        ChildLayout, LayoutInputKey, LayoutInputKind,
        builder::{BuildRequest, LayoutInputTreeBuilder},
        text::TextStyleBits,
        tree::LayoutInputTree,
    },
    style::bx::{self, DisplayInner, DisplayOuter},
};
use comet_tree::{node::Node, ui::Ui};

const VIEWPORT: Size<f64> = Size::new(800.0, 600.0);
const FULL: DirtyFlags = DirtyFlags::NEED_FULL_LAYOUT;
const CHILDREN: DirtyFlags = DirtyFlags::NEED_CHILDREN;

fn build(
    builder: &mut LayoutInputTreeBuilder,
    ui: &Ui,
    root: comet_tree::node::NodeKey,
    tree: &mut LayoutInputTree,
    dirty: &mut DirtyMap,
) -> Option<LayoutInputKey> {
    builder.build(ui, tree, dirty, BuildRequest::new(root, 16.0, VIEWPORT))
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
    let fresh_key = build(
        &mut LayoutInputTreeBuilder::new(),
        ui,
        root,
        &mut fresh,
        &mut DirtyMap::new(),
    )
    .unwrap();
    assert_eq!(shape(tree, key), shape(&fresh, fresh_key));
}

fn assert_clean(tree: &LayoutInputTree, dirty: &DirtyMap, root: LayoutInputKey) {
    for key in tree.items.descendants(root) {
        assert!(dirty.entry(key).flags.is_empty());
        assert!(dirty.entry(key).child_bits.is_empty());
    }
}

fn flags2(tree: &LayoutInputTree, dirty: &DirtyMap, node: comet_tree::node::NodeKey) -> DirtyFlags {
    dirty.entry(tree.item_for_node(node).unwrap()).flags
}

fn child_flags(
    tree: &LayoutInputTree,
    dirty: &DirtyMap,
    node: comet_tree::node::NodeKey,
) -> DirtyFlags {
    dirty.entry(tree.item_for_node(node).unwrap()).child_bits
}

#[test]
fn fresh_tree_mirrors_ui_with_full_layout_flags() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let span = ui.create(Node::Box, (DisplayOuter::Inline,));
    let text = ui.create(Node::Text, ());
    let block = ui.create(Node::Box, (DisplayOuter::Block,));
    let content = ui.create(
        Node::Box,
        (DisplayInner::Content(ContentId::from_ffi(NonZeroU64::MIN)),),
    );
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
    let mut dirty = DirtyMap::new();
    let key = build(
        &mut LayoutInputTreeBuilder::new(),
        &ui,
        root,
        &mut tree,
        &mut dirty,
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
        assert_eq!(dirty.entry(item).flags, FULL);
        assert_eq!(tree.item_for_node(tree.items[item].node), Some(item));
    }

    // Every item with children carries the full flags of its descendants.
    for item in tree.items.descendants(key) {
        let expected = match tree.items.first_child(item) {
            Some(_) => FULL,
            None => DirtyFlags::empty(),
        };
        assert_eq!(dirty.entry(item).child_bits, expected);
    }
    assert!(child_flags(&tree, &dirty, text).is_empty());

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
    let mut dirty = DirtyMap::new();
    let key = build(
        &mut LayoutInputTreeBuilder::new(),
        &ui,
        root,
        &mut tree,
        &mut dirty,
    )
    .unwrap();
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
    let mut dirty = DirtyMap::new();
    let key = build(
        &mut LayoutInputTreeBuilder::new(),
        &ui,
        root,
        &mut tree,
        &mut dirty,
    )
    .unwrap();
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
    dirty.clear(&tree, key);
    build(
        &mut LayoutInputTreeBuilder::new(),
        &ui,
        root,
        &mut tree,
        &mut dirty,
    )
    .unwrap();
    assert_eq!(bits(&tree, spaced), TextStyleBits::DEPENDS_VIEWPORT);
    assert_eq!(tree.viewport_dependents(), 4);
    assert_clean(&tree, &dirty, key);
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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    dirty.clear(&tree, key);

    tree.invalidate_viewport(&mut dirty, key);
    assert_eq!(flags2(&tree, &dirty, spaced), FULL);
    assert_eq!(flags2(&tree, &dirty, inherited), FULL);
    assert_eq!(flags2(&tree, &dirty, tall), FULL);
    assert!(flags2(&tree, &dirty, independent).is_empty());
    assert!(flags2(&tree, &dirty, plain).is_empty());
    assert_eq!(child_flags(&tree, &dirty, plain), FULL);
    assert_eq!(child_flags(&tree, &dirty, root), FULL);

    // An unmarked edit outside the viewport dependent subtrees is not seen.
    ui.set_props(independent, (bx::FontSize(StyleUnit::Px(30.0)),));
    let viewport = Size::new(400.0, 300.0);
    builder.build(
        &ui,
        &mut tree,
        &mut dirty,
        BuildRequest::new(root, 16.0, viewport),
    );

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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
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
    dirty.clear(&tree, key);
    ui.set_props(spaced, (bx::LetterSpacing(StyleUnit::Px(1.0)),));
    dirty.mark_node(&tree, spaced, FULL);
    build(&mut builder, &ui, root, &mut tree, &mut dirty);
    assert!(bits(&tree, spaced).is_empty());
    assert!(bits(&tree, inherited).is_empty());
    assert_eq!(tree.viewport_dependents(), 0);

    // Without dependents, invalidating the viewport marks nothing.
    dirty.clear(&tree, key);
    tree.invalidate_viewport(&mut dirty, key);
    assert_clean(&tree, &dirty, key);

    // New items are counted.
    let tall = ui.create(Node::Text, (bx::FontSize(StyleUnit::Vh(0.05)),));
    ui.append(plain, tall);
    dirty.mark_node(&tree, plain, CHILDREN);
    build(&mut builder, &ui, root, &mut tree, &mut dirty);
    assert_eq!(bits(&tree, tall), depends);
    assert_eq!(tree.viewport_dependents(), 1);

    // Deleted items are no longer counted.
    dirty.clear(&tree, key);
    ui.delete(tall);
    dirty.mark_node(&tree, plain, CHILDREN);
    build(&mut builder, &ui, root, &mut tree, &mut dirty);
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
    let mut dirty = DirtyMap::new();
    let key = build(
        &mut LayoutInputTreeBuilder::new(),
        &ui,
        root,
        &mut tree,
        &mut dirty,
    )
    .unwrap();
    dirty.clear(&tree, key);

    let leaf_key = tree.item_for_node(leaf).unwrap();
    dirty.mark(&tree, leaf_key, DirtyFlags::NEED_BOX_LAYOUT);

    assert_eq!(flags2(&tree, &dirty, leaf), DirtyFlags::NEED_BOX_LAYOUT);
    assert!(child_flags(&tree, &dirty, leaf).is_empty());
    assert!(flags2(&tree, &dirty, left).is_empty());
    assert_eq!(
        child_flags(&tree, &dirty, left),
        DirtyFlags::NEED_BOX_LAYOUT
    );
    assert_eq!(
        child_flags(&tree, &dirty, root),
        DirtyFlags::NEED_BOX_LAYOUT
    );
    assert!(child_flags(&tree, &dirty, right).is_empty());

    // Further flags are added to the ancestors too.
    dirty.mark_node(&tree, right, CHILDREN);
    assert_eq!(flags2(&tree, &dirty, right), CHILDREN);
    assert_eq!(
        child_flags(&tree, &dirty, root),
        DirtyFlags::NEED_BOX_LAYOUT | CHILDREN
    );
    assert_eq!(
        child_flags(&tree, &dirty, left),
        DirtyFlags::NEED_BOX_LAYOUT
    );

    // A full change includes the others.
    dirty.mark(&tree, leaf_key, FULL);
    assert_eq!(flags2(&tree, &dirty, leaf), FULL);
    assert_eq!(child_flags(&tree, &dirty, left), FULL);
    assert!(FULL.contains(DirtyFlags::NEED_BOX_LAYOUT | CHILDREN));

    // Unmapped nodes are ignored.
    let unmapped = ui.create(Node::Box, ());
    dirty.mark_node(&tree, unmapped, FULL);
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
    let mut dirty = DirtyMap::new();
    let key = build(
        &mut LayoutInputTreeBuilder::new(),
        &ui,
        root,
        &mut tree,
        &mut dirty,
    )
    .unwrap();

    // Clearing a subtree keeps the flags of its ancestors and siblings.
    dirty.clear(&tree, tree.item_for_node(left).unwrap());
    assert!(flags2(&tree, &dirty, left).is_empty());
    assert!(child_flags(&tree, &dirty, left).is_empty());
    assert!(flags2(&tree, &dirty, leaf).is_empty());
    assert_eq!(flags2(&tree, &dirty, right), FULL);
    assert_eq!(flags2(&tree, &dirty, root), FULL);

    dirty.clear(&tree, key);
    assert_clean(&tree, &dirty, key);
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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    dirty.clear(&tree, key);

    // Building again without marks changes nothing.
    assert_eq!(
        build(&mut builder, &ui, root, &mut tree, &mut dirty),
        Some(key)
    );
    assert_clean(&tree, &dirty, key);

    let a_key = tree.item_for_node(a).unwrap();
    let b_key = tree.item_for_node(b).unwrap();
    let inner_key = tree.item_for_node(inner).unwrap();

    // Reorder, delete, and insert.
    let d = ui.create(Node::Text, ());
    ui.before(a, b);
    ui.delete(c);
    ui.append(root, d);
    dirty.mark(&tree, key, CHILDREN);
    assert_eq!(
        build(&mut builder, &ui, root, &mut tree, &mut dirty),
        Some(key)
    );

    let children: Vec<_> = tree.items.children(key).collect();
    let d_key = tree.item_for_node(d).unwrap();

    assert_eq!(children, vec![b_key, a_key, d_key]);
    assert_eq!(tree.item_for_node(c), None);
    assert_eq!(tree.item_for_node(inner), Some(inner_key));

    // Only the new child is flagged; the flags are left for later steps.
    assert_eq!(dirty.entry(d_key).flags, FULL);
    assert!(dirty.entry(a_key).flags.is_empty());
    assert!(dirty.entry(b_key).flags.is_empty());
    assert!(dirty.entry(inner_key).flags.is_empty());
    // A child is gone, so the parent is dirty.
    assert_eq!(dirty.entry(key).flags, FULL);
    assert_eq!(dirty.entry(key).child_bits, FULL);
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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    dirty.clear(&tree, key);

    ui.set_props(child, (bx::FontSize(StyleUnit::Px(24.0)),));
    dirty.mark_node(&tree, child, FULL);
    build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();

    let item = |node| &tree.items[tree.item_for_node(node).unwrap()];
    let key_of = |node| tree.item_for_node(node).unwrap();

    assert_eq!(item(child).text_style.span.font_size, 24.0);
    assert_eq!(item(text).text_style.span.font_size, 24.0);
    // Its own font size wins.
    assert_eq!(item(fixed).text_style.span.font_size, 10.0);

    // Every item under the marked one is marked, and those whose font size
    // changed need pre-paint.
    let prepaint = FULL | DirtyFlags::NEED_PRE_PAINT;
    assert_eq!(dirty.entry(key_of(child)).flags, prepaint);
    assert_eq!(dirty.entry(key_of(text)).flags, prepaint);
    assert_eq!(dirty.entry(key_of(fixed)).flags, FULL);
    assert!(dirty.entry(key_of(root)).flags.is_empty());
    assert_eq!(dirty.entry(key_of(root)).child_bits, prepaint);
    // The unmarked sibling was skipped.
    assert!(dirty.entry(key_of(sibling)).flags.is_empty());

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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    dirty.clear(&tree, key);

    // A display change resolving to the same style and child layout marks
    // no child.
    ui.set_props(outer, (DisplayOuter::Block,));
    dirty.mark_node(&tree, outer, FULL);
    build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();

    assert_eq!(flags2(&tree, &dirty, outer), FULL);
    assert!(flags2(&tree, &dirty, inner).is_empty());
    assert!(flags2(&tree, &dirty, text).is_empty());
    assert!(child_flags(&tree, &dirty, outer).is_empty());
    assert_fresh(&ui, root, &tree, key);
    dirty.clear(&tree, key);

    // A changed font size cascades until a child sets its own: the inner box
    // resolves as before, so its children keep their flags, but a marked
    // child under it is still resolved.
    ui.set_props(outer, (bx::FontSize(StyleUnit::Px(24.0)),));
    ui.set_props(marked, (bx::FontSize(StyleUnit::Px(12.0)),));
    dirty.mark_node(&tree, outer, FULL);
    dirty.mark_node(&tree, marked, FULL);
    build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();

    let item = |node| &tree.items[tree.item_for_node(node).unwrap()];
    let key_of = |node| tree.item_for_node(node).unwrap();

    assert_eq!(item(outer).text_style.span.font_size, 24.0);
    assert_eq!(dirty.entry(key_of(inner)).flags, FULL);
    assert_eq!(item(inner).text_style.span.font_size, 10.0);
    assert!(dirty.entry(key_of(text)).flags.is_empty());
    assert_eq!(
        dirty.entry(key_of(marked)).flags,
        FULL | DirtyFlags::NEED_PRE_PAINT
    );
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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    dirty.clear(&tree, key);

    let child_key = tree.item_for_node(child).unwrap();
    ui.set_props(child, (bx::FontSize(StyleUnit::Em(2.0)),));
    dirty.mark(&tree, child_key, FULL);

    assert_eq!(
        build(&mut builder, &ui, child, &mut tree, &mut dirty),
        Some(child_key)
    );
    assert_eq!(tree.items[child_key].text_style.span.font_size, 40.0);

    // A new node is inserted by updating the item of its parent.
    dirty.clear(&tree, key);
    let other_child = ui.create(Node::Text, ());
    ui.append(root, other_child);
    dirty.mark_node(&tree, root, CHILDREN);
    assert_eq!(
        build(&mut builder, &ui, root, &mut tree, &mut dirty),
        Some(key)
    );

    let inserted = tree.item_for_node(other_child).unwrap();
    assert_eq!(tree.items.parent(inserted), Some(key));
    assert_eq!(dirty.entry(inserted).flags, FULL);
    assert_eq!(tree.items[inserted].text_style.span.font_size, 20.0);
    assert_eq!(tree.item_for_node(child), Some(child_key));
    assert!(dirty.entry(child_key).flags.is_empty());

    // A descendant without a parent item means the tree does not mirror the UI.
    let mut other = LayoutInputTree::new();
    assert_eq!(
        build(&mut builder, &ui, child, &mut other, &mut DirtyMap::new()),
        None
    );
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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    let paragraph_key = tree.item_for_node(paragraph).unwrap();
    let span_key = tree.item_for_node(span).unwrap();
    let interruption_key = tree.item_for_node(interruption).unwrap();

    // Child layouts come from each node's own display, not from its content.
    assert_eq!(tree.items[paragraph_key].child_layout, ChildLayout::Block);
    assert_eq!(tree.items[span_key].child_layout, ChildLayout::Inline);
    dirty.clear(&tree, key);

    ui.set_props(interruption, (DisplayOuter::Inline, DisplayInner::FlowRoot));
    dirty.mark_node(&tree, interruption, FULL);
    assert_eq!(
        build(&mut builder, &ui, interruption, &mut tree, &mut dirty),
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
    dirty.clear(&tree, key);
    ui.delete(interruption);
    assert_eq!(
        build(&mut builder, &ui, interruption, &mut tree, &mut dirty),
        None
    );
    assert_eq!(tree.item_for_node(interruption), None);
    assert_eq!(dirty.entry(span_key).flags, FULL);
    assert_eq!(dirty.entry(paragraph_key).child_bits, FULL);
    assert_eq!(dirty.entry(key).child_bits, FULL);
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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    let child_key = tree.item_for_node(child).unwrap();
    let text_key = tree.item_for_node(text).unwrap();
    dirty.clear(&tree, key);

    // Unmarked items keep the units they were resolved with.
    let viewport = Size::new(1000.0, 600.0);
    assert_eq!(
        builder.build(
            &ui,
            &mut tree,
            &mut dirty,
            BuildRequest::new(root, 20.0, viewport)
        ),
        Some(key)
    );
    assert_eq!(tree.items[child_key].text_style.span.font_size, 32.0);
    assert_eq!(tree.items[text_key].text_style.span.letter_spacing, 8.0);
    assert_clean(&tree, &dirty, key);

    // Marked items resolve with the request.
    dirty.mark(&tree, text_key, FULL);
    builder.build(
        &ui,
        &mut tree,
        &mut dirty,
        BuildRequest::new(root, 16.0, viewport),
    );
    assert_eq!(tree.items[text_key].text_style.span.letter_spacing, 10.0);
    assert_eq!(dirty.entry(text_key).flags, FULL);
    assert_eq!(dirty.entry(child_key).child_bits, FULL);

    // Flags the builder does not consume are kept.
    dirty.clear(&tree, key);
    dirty.mark(&tree, text_key, DirtyFlags::NEED_BOX_LAYOUT);
    builder.build(
        &ui,
        &mut tree,
        &mut dirty,
        BuildRequest::new(root, 16.0, viewport),
    );
    assert_eq!(dirty.entry(text_key).flags, DirtyFlags::NEED_BOX_LAYOUT);
}

#[test]
fn build_builds_a_new_tree_without_a_source_item() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Box, (bx::FontSize(StyleUnit::Rem(2.0)),));
    ui.append(root, child);

    let mut tree = LayoutInputTree::new();
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();

    // A new root font size needs a new tree.
    tree.delete_item(key);
    let rebuilt = builder
        .build(
            &ui,
            &mut tree,
            &mut dirty,
            BuildRequest::new(root, 20.0, VIEWPORT),
        )
        .unwrap();

    assert_eq!(tree.items.descendants(rebuilt).count(), 2);
    for item in tree.items.descendants(rebuilt) {
        assert_eq!(dirty.entry(item).flags, FULL);
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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    let a_key = tree.item_for_node(a).unwrap();
    let b_key = tree.item_for_node(b).unwrap();
    dirty.clear(&tree, key);

    // Insert under a nested item, and reorder and insert under the root.
    let text = ui.create(Node::Text, ());
    let nested = ui.create(Node::Text, ());
    ui.append(root, text);
    ui.before(a, b);
    ui.append(inner, nested);
    dirty.mark(&tree, key, CHILDREN);
    dirty.mark_node(&tree, inner, CHILDREN);
    assert_eq!(
        build(&mut builder, &ui, root, &mut tree, &mut dirty),
        Some(key)
    );

    let text_key = tree.item_for_node(text).unwrap();
    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        vec![b_key, a_key, text_key]
    );
    assert_eq!(dirty.entry(text_key).flags, FULL);
    assert_eq!(flags2(&tree, &dirty, nested), FULL);
    assert!(dirty.entry(a_key).flags.is_empty());
    assert_eq!(dirty.entry(a_key).child_bits, FULL);
    assert!(dirty.entry(b_key).flags.is_empty());
    assert_fresh(&ui, root, &tree, key);

    // Deleting a node marks its parent.
    dirty.clear(&tree, key);
    ui.delete(a);
    dirty.mark(&tree, key, CHILDREN);
    build(&mut builder, &ui, root, &mut tree, &mut dirty);
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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    dirty.clear(&tree, key);

    // Content display has no child items.
    ui.set_props(
        parent,
        (DisplayInner::Content(ContentId::from_ffi(NonZeroU64::MIN)),),
    );
    dirty.mark_node(&tree, parent, FULL);
    build(&mut builder, &ui, root, &mut tree, &mut dirty);
    assert_eq!(tree.item_for_node(text), None);
    assert_fresh(&ui, root, &tree, key);

    // Back to flow, the child is built again.
    dirty.clear(&tree, key);
    ui.remove_prop::<DisplayInner>(parent);
    dirty.mark_node(&tree, parent, FULL);
    build(&mut builder, &ui, root, &mut tree, &mut dirty);
    let text_key = tree.item_for_node(text).unwrap();
    assert_eq!(dirty.entry(text_key).flags, FULL);
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
    let mut dirty = DirtyMap::new();
    let mut builder = LayoutInputTreeBuilder::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    let keys: Vec<_> = nodes
        .iter()
        .map(|&node| tree.item_for_node(node).unwrap())
        .collect();
    dirty.clear(&tree, key);

    ui.delete(nodes[0]);
    dirty.mark(&tree, key, CHILDREN);
    build(&mut builder, &ui, root, &mut tree, &mut dirty);

    assert_eq!(tree.items.children(key).collect::<Vec<_>>(), keys[1..]);
    for &item in &keys[1..] {
        assert!(dirty.entry(item).flags.is_empty());
    }
    assert_eq!(dirty.entry(key).flags, FULL);

    dirty.clear(&tree, key);
    ui.prepend(root, nodes[3]);
    dirty.mark(&tree, key, CHILDREN);
    build(&mut builder, &ui, root, &mut tree, &mut dirty);

    assert_eq!(
        tree.items.children(key).collect::<Vec<_>>(),
        vec![keys[3], keys[1], keys[2]]
    );
    for &item in &keys[1..] {
        assert!(dirty.entry(item).flags.is_empty());
    }
}

#[test]
fn only_font_size_changes_mark_prepaint() {
    const PRE_PAINT: DirtyFlags = DirtyFlags::NEED_PRE_PAINT;

    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let parent = ui.create(Node::Box, ());
    let child = ui.create(Node::Text, ());
    let sized = ui.create(Node::Text, (bx::FontSize(StyleUnit::Px(20.0)),));
    ui.append(root, parent);
    ui.append(parent, child);
    ui.append(parent, sized);

    let mut builder = LayoutInputTreeBuilder::new();
    let mut tree = LayoutInputTree::new();
    let mut dirty = DirtyMap::new();
    let key = build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    dirty.clear(&tree, key);

    // A text style change keeping the font size marks no pre-paint.
    ui.set_props(parent, (bx::LetterSpacing(StyleUnit::Px(2.0)),));
    dirty.mark_node(&tree, parent, FULL);
    build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    for node in [parent, child, sized] {
        assert!(!flags2(&tree, &dirty, node).contains(PRE_PAINT), "{node:?}");
    }
    dirty.clear(&tree, key);

    // A font size change marks the item and the descendants inheriting it,
    // not those setting their own.
    ui.set_props(parent, (bx::FontSize(StyleUnit::Px(24.0)),));
    dirty.mark_node(&tree, parent, FULL);
    build(&mut builder, &ui, root, &mut tree, &mut dirty).unwrap();
    assert!(flags2(&tree, &dirty, parent).contains(PRE_PAINT));
    assert!(flags2(&tree, &dirty, child).contains(PRE_PAINT));
    assert!(!flags2(&tree, &dirty, sized).contains(PRE_PAINT));
    assert!(child_flags(&tree, &dirty, root).contains(PRE_PAINT));
}
