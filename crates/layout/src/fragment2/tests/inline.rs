use slotmap::SlotMap;

use crate::{
    fragment2::{
        FragmentGeometry, InlineItem, InlineItemKind, InlineLayout, InlineLink, OffsetEntry,
        inline_box_id, inline_box_source,
    },
    input::LayoutInputKey,
};

fn item(source: LayoutInputKey, kind: InlineItemKind, descendants: u32) -> InlineItem {
    InlineItem {
        source,
        kind,
        descendants,
        link: InlineLink::default(),
        geometry: FragmentGeometry::default(),
        baseline: 0.0,
    }
}

#[test]
fn traverses_subtrees_and_links() {
    let mut keys = SlotMap::<LayoutInputKey, ()>::with_key();
    let (owner, span, text) = (keys.insert(()), keys.insert(()), keys.insert(()));
    let piece = InlineItemKind::Box { child: None };
    let mut layout = InlineLayout::new();

    // line { span { run } } line { span { run } run }
    layout.items = vec![
        item(owner, InlineItemKind::Line { line: 0 }, 2),
        item(span, piece, 1),
        item(text, InlineItemKind::Run { line: 0, run: 0 }, 0),
        item(owner, InlineItemKind::Line { line: 1 }, 3),
        item(span, piece, 1),
        item(text, InlineItemKind::Run { line: 1, run: 0 }, 0),
        item(text, InlineItemKind::Run { line: 1, run: 1 }, 0),
    ];
    layout.items[1].link.next = 3;
    layout.items[4].link.prev = -3;
    layout.items[2].link.next = 3;
    layout.items[5].link = InlineLink { prev: -3, next: 1 };
    layout.items[6].link.prev = -1;

    assert_eq!(layout.roots().collect::<Vec<_>>(), [0, 3]);
    assert_eq!(layout.children(3).collect::<Vec<_>>(), [4, 6]);
    assert_eq!(layout.subtree(3), 4..7);
    assert_eq!(layout.items_of(span).collect::<Vec<_>>(), [1, 4]);
    assert_eq!(layout.items_of(text).collect::<Vec<_>>(), [2, 5, 6]);
    assert_eq!(layout.prev(5), Some(2));
    assert_eq!(layout.items_of(owner).next(), None);
}

#[test]
fn finds_text_items_around_inline_elements() {
    let mut keys = SlotMap::<LayoutInputKey, ()>::with_key();
    let (a, span, b, c) = (
        keys.insert(()),
        keys.insert(()),
        keys.insert(()),
        keys.insert(()),
    );
    let mut layout = InlineLayout::new();
    // "ab" | span("cd" "ef") | box at 6 | "g" from 7
    layout.offset_mapping = vec![
        OffsetEntry {
            source: a,
            range: 0..2,
            text: true,
        },
        OffsetEntry {
            source: span,
            range: 2..6,
            text: false,
        },
        OffsetEntry {
            source: b,
            range: 2..6,
            text: true,
        },
        OffsetEntry {
            source: c,
            range: 7..8,
            text: true,
        },
    ];

    let down = parley::Affinity::Downstream;
    let up = parley::Affinity::Upstream;

    assert_eq!(layout.item_at(1, down), Some(a));
    assert_eq!(layout.item_at(2, down), Some(b));
    assert_eq!(layout.item_at(2, up), Some(a));
    assert_eq!(layout.item_at(6, down), Some(b), "inline box after b");
    assert_eq!(layout.item_at(7, up), Some(c), "nothing ends at 7");
    assert_eq!(layout.item_at(99, down), Some(c));
    assert_eq!(layout.range(span), Some(2..6));
}

#[test]
fn inline_box_ids_round_trip_to_items() {
    let mut keys = SlotMap::<LayoutInputKey, ()>::with_key();
    let (a, b) = (keys.insert(()), keys.insert(()));
    keys.remove(a);
    let c = keys.insert(());

    for key in [b, c] {
        assert_eq!(inline_box_source(inline_box_id(key)), key);
    }
    assert_ne!(inline_box_id(a), inline_box_id(c), "versions differ");
}

#[test]
fn hits_items_inside_their_border_boxes() {
    let mut keys = SlotMap::<LayoutInputKey, ()>::with_key();
    let (owner, text) = (keys.insert(()), keys.insert(()));
    let mut layout = InlineLayout::new();
    let mut run = item(text, InlineItemKind::Run { line: 0, run: 0 }, 0);
    run.geometry.location = crate::style::geometry::Point::new(10.0, 4.0);
    run.geometry.size = crate::style::geometry::Size::new(20.0, 8.0);
    layout.items = vec![item(owner, InlineItemKind::Line { line: 0 }, 1), run];

    let hit = |x, y| layout.hit_local(1, kurbo::Point::new(x, y));

    // Placed at its location in the owner's border box.
    assert!(hit(10.0, 4.0));
    assert!(hit(29.9, 11.9));
    assert!(!hit(30.0, 8.0));
    assert!(!hit(9.9, 8.0));
    assert!(!hit(20.0, 12.0));

    // An empty line box covers nothing, and missing items are never hit.
    assert!(!layout.hit_local(0, kurbo::Point::new(0.0, 0.0)));
    assert!(!layout.hit_local(2, kurbo::Point::new(15.0, 8.0)));
}
