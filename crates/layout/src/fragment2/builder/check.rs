//! Consistency checks of a laid out fragment tree, in debug builds.

use crate::{
    fragment2::{BoxFragmentKey, BoxFragmentType, FragmentTree},
    input::tree::LayoutInputTree,
};

use super::classify::ChildKind;

/// Check the boxes of `root` once laid out:
/// - every box is the box of its source, and its content layout kind
///   matches its source;
/// - a built inline layout has exactly the children of its box as the boxes
///   inside, the descendant counts of its items stay within their lines, and
///   its links pair up;
/// - only boxes inside inline content are inline parts or atomic inlines.
pub(super) fn check(fragments: &FragmentTree, tree: &LayoutInputTree, root: BoxFragmentKey) {
    for key in fragments.boxes.descendants(root) {
        let fragment = &fragments.boxes[key];
        let source = fragment.source;

        assert_eq!(
            fragments.box_fragment(source),
            Some(key),
            "a box is the box of its source"
        );
        assert_eq!(
            ChildKind::of(&fragment.child_layout),
            ChildKind::of_item(tree, source),
            "the content layout of a box follows its source"
        );

        let in_inline = fragments
            .boxes
            .parent(key)
            .is_some_and(|parent| fragments.boxes[parent].child_layout.is_inline());
        if matches!(
            fragment.box_type,
            BoxFragmentType::InlinePart | BoxFragmentType::AtomicInline
        ) {
            assert!(in_inline, "inline level boxes are inside inline content");
        }

        let Some(inline) = fragment.inline().filter(|inline| inline.is_built()) else {
            continue;
        };

        let mut atomics: Vec<_> = inline.atomics.values().copied().collect();
        let mut children: Vec<_> = fragments.boxes.children(key).collect();
        atomics.sort();
        children.sort();
        assert_eq!(atomics, children, "the boxes inside are the children");

        for (&item, &child) in &inline.atomics {
            assert_eq!(fragments.boxes[child].source, item);
        }

        let len = inline.items.len();
        let mut index = 0;
        while index < len {
            assert!(
                inline.items[index].is_line(),
                "lines are the top level items"
            );
            let end = index + 1 + inline.items[index].descendants as usize;
            assert!(end <= len, "a line covers items of the layout");

            for (inner, item) in inline.items.iter().enumerate().take(end).skip(index + 1) {
                assert!(!item.is_line(), "lines do not nest");
                assert!(
                    inner + 1 + item.descendants as usize <= end,
                    "items stay within their line"
                );
            }

            index = end;
        }

        for index in 0..len {
            if let Some(next) = inline.next(index) {
                assert_eq!(inline.prev(next), Some(index), "links pair up");
                assert_eq!(inline.items[next].source, inline.items[index].source);
            }
        }
    }
}
