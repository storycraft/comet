use slotmap::DefaultKey;

use crate::tree::SlotTree;

#[test]
fn test_child() {
    let mut tree = SlotTree::<DefaultKey, i32>::new();

    /*
     * Create a following tree
     * a
     * ├─ c
     * ├─ d
     * └─ b
     */
    let a = tree.insert(1);
    let b = tree.insert(2);
    let c = tree.insert(3);
    let d = tree.insert(4);

    tree.append(a, b);
    tree.before(b, c);
    tree.after(c, d);
    assert_eq!(tree.cursor(Some(a)).skip(1).next(), None);

    let mut child_cursor = tree.cursor(Some(b));
    assert_eq!(child_cursor.next_back(), Some(b));
    assert_eq!(child_cursor.next_back(), Some(d));

    let mut child_cursor = child_cursor.skip(2);
    assert_eq!(child_cursor.next(), Some(b));
    assert_eq!(child_cursor.next(), None);
}