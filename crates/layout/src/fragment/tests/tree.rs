use crate::{
    fragment::{BoxFragment, BoxFragmentType, ChildLayoutType, tree::FragmentTree},
    input::LayoutInputKey,
    style::LayoutConstraint,
    taffy::style,
};
use comet_geometry::Size;
use slotmap::SlotMap;

#[test]
fn delete_removes_source_lookups() {
    let mut keys = SlotMap::<LayoutInputKey, ()>::with_key();
    let (a, b) = (keys.insert(()), keys.insert(()));
    let mut tree = FragmentTree::new();
    let cx = LayoutConstraint::new_root(Size::new(100.0, 100.0), 16.0);
    let new = |source| {
        BoxFragment::new(
            source,
            BoxFragmentType::Normal,
            ChildLayoutType::Block,
            style::resolve(None, &cx),
        )
    };

    let root = tree.insert(new(a));
    let child = tree.insert(new(b));
    tree.boxes.append(root, child);

    assert_eq!(tree.box_fragment(b), Some(child));
    assert!(tree.delete(root).is_some());
    assert_eq!(tree.box_fragment(a), None);
    assert_eq!(tree.box_fragment(b), None);
    assert!(tree.boxes.is_empty());
}

#[test]
fn boxes_outside_source_lookup_are_anonymous() {
    let mut keys = SlotMap::<LayoutInputKey, ()>::with_key();
    let a = keys.insert(());
    let mut tree = FragmentTree::new();
    let cx = LayoutConstraint::new_root(Size::new(100.0, 100.0), 16.0);
    let new = || {
        BoxFragment::new(
            a,
            BoxFragmentType::Normal,
            ChildLayoutType::Block,
            style::resolve(None, &cx),
        )
    };

    let root = tree.insert(new());
    let anonymous = tree.insert_anonymous(a, ChildLayoutType::Block, style::resolve(None, &cx));
    tree.boxes.append(root, anonymous);

    assert!(!tree.is_anonymous(root));
    assert!(tree.is_anonymous(anonymous));
    assert_eq!(tree.boxes[anonymous].box_type, BoxFragmentType::Anonymous);
    assert_eq!(tree.box_fragment(a), Some(root));

    // Deleting the anonymous box keeps the box of its source.
    tree.delete(anonymous);
    assert_eq!(tree.box_fragment(a), Some(root));
    assert!(!tree.is_anonymous(anonymous), "not in the tree");
}
