//! Consistency checks of a laid out fragment tree, in debug builds.

use super::classify::ChildKind;
use crate::fragment2::{
    BoxFragmentKey, BoxFragmentLink, BoxFragmentType, FragmentTree,
    inline::collect::InlineBuildContext,
};

/// Check the boxes of `root` once laid out:
/// - every box is the box of its source, or an anonymous run box of a mixed
///   inline box, and its content layout kind matches its source;
/// - the runs of a mixed inline box are never adjacent, and are linked in
///   order; other boxes are not linked;
/// - a built inline layout has exactly the children of its box as the boxes
///   inside, the descendant counts of its items stay within their lines, and
///   its links pair up;
/// - only boxes inside inline content are atomic inlines or inline parts,
///   and the links of inline parts pair up within one element.
pub(super) fn check(cx: &InlineBuildContext<'_>, fragments: &FragmentTree, root: BoxFragmentKey) {
    for key in fragments.boxes.descendants(root) {
        let fragment = &fragments.boxes[key];
        let source = fragment.source;
        let parent = fragments.boxes.parent(key);

        if fragment.box_type == BoxFragmentType::InlinePart {
            assert!(
                fragments.is_anonymous(key),
                "inline part boxes are anonymous"
            );
            assert!(
                parent.is_some_and(|parent| fragments.boxes[parent].child_layout.is_inline()),
                "inline part boxes are inside inline layouts"
            );
            assert!(fragment.child_layout.is_content());
        } else if fragments.is_anonymous(key) {
            assert_eq!(
                fragment.box_type,
                BoxFragmentType::Anonymous,
                "anonymous boxes other than inline parts are marked anonymous"
            );
            let owner = parent.map(|parent| &fragments.boxes[parent]);
            assert!(
                owner.is_some_and(|owner| owner.child_layout.is_mixed_inline()),
                "anonymous boxes are runs of mixed inline boxes"
            );
            assert_eq!(owner.map(|owner| owner.source), Some(source));
            assert!(fragment.child_layout.is_inline(), "runs are inline layouts");
        } else {
            assert_eq!(
                fragments.box_fragment(source),
                Some(key),
                "a box is the box of its source"
            );
            assert_eq!(
                ChildKind::of(&fragment.child_layout),
                ChildKind::of_item(cx, source),
                "the content layout of a box follows its source"
            );
            assert_eq!(
                fragment.link,
                BoxFragmentLink::default(),
                "only split boxes are linked"
            );
        }

        if fragment.box_type == BoxFragmentType::InlinePart {
            check_piece_links(fragments, key);
        }

        let in_inline =
            parent.is_some_and(|parent| fragments.boxes[parent].child_layout.is_inline());
        if fragment.box_type == BoxFragmentType::AtomicInline {
            assert!(in_inline, "inline level boxes are inside inline content");
        }

        if fragment.child_layout.is_mixed_inline() {
            check_runs(fragments, key);
            assert_eq!(
                fragment.child_layout.first_run(),
                fragments
                    .boxes
                    .children(key)
                    .find(|&child| fragments.is_anonymous(child)),
                "a mixed inline box holds its first run"
            );
        }

        let Some(inline) = fragment.inline().filter(|inline| inline.is_built()) else {
            continue;
        };

        // The children are the boxes inside, and the boxes of the pieces.
        let mut expected: Vec<_> = inline.atomics.values().copied().collect();
        for item in &inline.items {
            if let Some(child) = item.child_box()
                && fragments.boxes[child].box_type == BoxFragmentType::InlinePart
            {
                assert_eq!(
                    fragments.boxes[child].layout.geometry, item.geometry,
                    "an inline part box covers its piece"
                );
                expected.push(child);
            }
        }
        let mut children: Vec<_> = fragments.boxes.children(key).collect();
        expected.sort();
        children.sort();
        assert_eq!(expected, children, "the boxes inside are the children");

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

/// The runs of the mixed inline box `owner` are never adjacent, and form one
/// chain in child order.
fn check_runs(fragments: &FragmentTree, owner: BoxFragmentKey) {
    let mut prev_run: Option<BoxFragmentKey> = None;
    let mut prev_was_run = false;

    for child in fragments.boxes.children(owner) {
        let run = fragments.is_anonymous(child);
        assert!(!(run && prev_was_run), "runs are never adjacent");
        prev_was_run = run;

        if run {
            let link = fragments.boxes[child].link;
            assert_eq!(link.prev, prev_run, "runs are linked in order");

            if let Some(prev) = prev_run {
                assert_eq!(fragments.boxes[prev].link.next, Some(child));
            }

            prev_run = Some(child);
        }
    }

    if let Some(last) = prev_run {
        assert_eq!(fragments.boxes[last].link.next, None);
    }
}

/// The links of the inline part box `key` pair up with those of the pieces
/// of the same element around it.
fn check_piece_links(fragments: &FragmentTree, key: BoxFragmentKey) {
    let fragment = &fragments.boxes[key];

    let pairs = [(fragment.link.prev, true), (fragment.link.next, false)];

    for (other, prev) in pairs {
        let Some(other) = other else {
            continue;
        };
        let other = &fragments.boxes[other];
        let back = if prev {
            other.link.next
        } else {
            other.link.prev
        };

        assert_eq!(other.source, fragment.source, "pieces of one element");
        assert_eq!(other.box_type, BoxFragmentType::InlinePart);
        assert_eq!(back, Some(key), "piece links pair up");
    }
}
