use crate::tree::ArchetypalTree;

#[test]
fn test_child() {
    let mut tree = ArchetypalTree::new();

    /*
     * Create a following tree
     * a
     * ├─ c
     * ├─ d
     * └─ b
     */
    let a = tree.spawn((1,));
    let b = tree.spawn((2,));
    let c = tree.spawn((3,));
    let d = tree.spawn((4,));

    tree.append(a, b);
    tree.before(b, c);
    tree.after(c, d);
    assert_eq!(tree.cursor(Some(a)).nth(1), None);

    let mut child_cursor = tree.cursor(Some(b));
    assert_eq!(child_cursor.next_back(), Some(b));
    assert_eq!(child_cursor.next_back(), Some(d));

    let mut child_cursor = child_cursor.skip(2);
    assert_eq!(child_cursor.next(), Some(b));
    assert_eq!(child_cursor.next(), None);
}
