use comet_tree::node::Text;
use comet_tree::prelude::*;

#[test]
fn repeated_moves_match_a_list_model() {
    let mut ui = Ui::new();
    let parents = [ui.create(Node::Box, ()), ui.create(Node::Box, ())];
    let keys: Vec<_> = (0..12).map(|_| ui.create(Node::Box, ())).collect();
    let mut model = [keys.clone(), vec![]];

    for &key in &keys {
        ui.append(parents[0], key);
    }

    let mut seed = 7_u32;
    for step in 0..1000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let key = keys[(seed as usize >> 8) % keys.len()];
        let destination = (seed as usize >> 20) % 2;
        let target = model[destination]
            .get((seed as usize >> 4) % model[destination].len().max(1))
            .copied();
        let old = model.iter().position(|list| list.contains(&key)).unwrap();
        let operation = step % 4;

        if operation >= 2 && target.is_none() {
            continue;
        }

        let returned = match operation {
            0 => ui.append(parents[destination], key),
            1 => ui.prepend(parents[destination], key),
            2 => ui.before(target.unwrap(), key),
            _ => ui.after(target.unwrap(), key),
        };
        assert_eq!(returned, Some(parents[old]));

        if operation < 2 || target != Some(key) {
            model[old].retain(|&entry| entry != key);

            let index = match operation {
                0 => model[destination].len(),
                1 => 0,
                _ => {
                    model[destination]
                        .iter()
                        .position(|&entry| Some(entry) == target)
                        .unwrap()
                        + usize::from(operation == 3)
                }
            };
            model[destination].insert(index, key);
        }

        for i in 0..2 {
            check(&ui, parents[i], &model[i]);
        }
    }
}

#[test]
fn deleting_deep_subtree_does_not_recurse() {
    let mut ui = Ui::new();
    let mut root = ui.create(Node::Box, ());
    let leaf = root;

    // Build upwards to avoid quadratic ancestor validation while constructing.
    for _ in 0..10000 {
        let parent = ui.create(Node::Box, ());
        ui.append(parent, root);
        root = parent;
    }

    ui.delete(root);

    assert!(ui.node(root).is_none());
    assert!(ui.node(leaf).is_none());
}

define_style_props! {
    #[derive(Debug, PartialEq)]
    pub Width: u32 = 10,
}

struct Color(u32);
style_prop!(Color);

fn check(ui: &Ui, parent: NodeKey, children: &[NodeKey]) {
    assert_eq!(ui.first_child(parent), children.first().copied());
    assert_eq!(ui.last_child(parent), children.last().copied());
    assert_eq!(ui.children(parent).collect::<Vec<_>>(), children);
    assert_eq!(
        ui.children(parent).rev().collect::<Vec<_>>(),
        children.iter().rev().copied().collect::<Vec<_>>()
    );

    for (i, &child) in children.iter().enumerate() {
        assert_eq!(ui.parent(child), Some(parent));
        assert_eq!(
            ui.prev_sibling(child),
            i.checked_sub(1).map(|j| children[j])
        );
        assert_eq!(ui.next_sibling(child), children.get(i + 1).copied());
    }
}

#[test]
fn insertion_and_same_parent_reordering() {
    let mut ui = UI::new();
    let p = ui.create(Node::Box, ());
    let a = ui.create(Node::Text, ());
    let b = ui.create(Node::Box, ());
    let c = ui.create(Node::Box, ());
    let d = ui.create(Node::Box, ());

    ui.prepend(p, b);
    check(&ui, p, &[b]);

    ui.prepend(p, a);
    ui.after(b, d);
    ui.before(d, c);
    check(&ui, p, &[a, b, c, d]);

    assert_eq!(ui.append(p, a), Some(p));
    check(&ui, p, &[b, c, d, a]);

    ui.append(p, a);
    ui.prepend(p, b);
    check(&ui, p, &[b, c, d, a]);

    ui.before(b, a);
    check(&ui, p, &[a, b, c, d]);

    ui.after(c, b);
    check(&ui, p, &[a, c, b, d]);

    ui.before(b, c);
    ui.after(b, b);
    ui.before(c, c);
    check(&ui, p, &[a, c, b, d]);
}

#[test]
fn reparent_and_detach_all_positions() {
    for position in 0..3 {
        let mut ui = Ui::new();
        let p = ui.create(Node::Box, ());
        let q = ui.create(Node::Box, ());
        let children: Vec<_> = (0..3).map(|_| ui.create(Node::Box, ())).collect();

        for &child in &children {
            ui.append(p, child);
        }

        assert_eq!(ui.prepend(q, children[position]), Some(p));
        let remaining: Vec<_> = children
            .iter()
            .copied()
            .filter(|&v| v != children[position])
            .collect();
        check(&ui, p, &remaining);
        check(&ui, q, &[children[position]]);

        assert_eq!(ui.remove_parent(children[position]), Some(q));
        check(&ui, q, &[]);
        assert_eq!(ui.parent(children[position]), None);
        assert_eq!(ui.prev_sibling(children[position]), None);
        assert_eq!(ui.next_sibling(children[position]), None);
    }
}

#[test]
fn double_ended_cursor_consumes_one_range() {
    let mut ui = Ui::new();
    let p = ui.create(Node::Box, ());
    let children: Vec<_> = (0..5).map(|_| ui.create(Node::Box, ())).collect();

    for &child in &children {
        ui.append(p, child);
    }

    let mut cursor = ui.cursor(Some(children[1]));
    assert_eq!(cursor.next_back(), Some(children[4]));
    assert_eq!(cursor.next(), Some(children[1]));
    assert_eq!(cursor.next_back(), Some(children[3]));
    assert_eq!(cursor.next(), Some(children[2]));

    assert_eq!(cursor.next_back(), None);
    assert_eq!(cursor.next(), None);

    assert_eq!(ui.cursor(None).next_back(), None);
    assert_eq!(ui.cursor(Some(p)).rev().collect::<Vec<_>>(), [p]);
}

#[test]
fn invalid_keys_return_quietly() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Text, (Text("hello".into()),));
    let invalid = ui.create(Node::Box, ());

    ui.append(root, child);
    ui.delete(invalid);

    assert!(ui.node(invalid).is_none());
    assert!(ui.props(invalid).is_none());
    assert!(ui.prop::<Text>(invalid).is_none());
    assert!(ui.prop_mut::<Text>(invalid).is_none());
    assert!(ui.remove_prop::<Text>(invalid).is_none());

    assert_eq!(ui.parent(invalid), None);
    assert_eq!(ui.first_child(invalid), None);
    assert_eq!(ui.last_child(invalid), None);
    assert_eq!(ui.next_sibling(invalid), None);
    assert_eq!(ui.prev_sibling(invalid), None);
    assert_eq!(ui.remove_parent(invalid), None);
    assert_eq!(ui.children(invalid).next(), None);
    assert_eq!(ui.cursor(Some(invalid)).next_back(), None);

    assert_eq!(ui.append(root, invalid), None);
    assert_eq!(ui.append(invalid, child), None);
    assert_eq!(ui.prepend(root, invalid), None);
    assert_eq!(ui.prepend(invalid, child), None);
    assert_eq!(ui.before(child, invalid), None);
    assert_eq!(ui.before(invalid, child), None);
    assert_eq!(ui.after(child, invalid), None);
    assert_eq!(ui.after(invalid, child), None);

    ui.set_props(invalid, (Text("ignored".into()),));
    ui.delete(invalid);

    check(&ui, root, &[child]);
    assert_eq!(ui.prop::<Text>(child).unwrap().0, "hello");
}

#[test]
fn invalid_moves_preserve_the_tree() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Box, ());
    let leaf = ui.create(Node::Text, ());
    let stale = ui.create(Node::Box, ());

    ui.delete(stale);
    ui.append(root, child);
    ui.append(child, leaf);

    ui.append(root, root);
    ui.prepend(leaf, root);
    ui.before(leaf, root);
    ui.after(leaf, root);

    ui.append(root, stale);
    ui.prepend(stale, child);
    ui.before(stale, child);
    ui.after(child, stale);

    ui.before(root, leaf);
    ui.after(root, leaf);

    check(&ui, root, &[child]);
    check(&ui, child, &[leaf]);
    check(&ui, leaf, &[]);
    assert_eq!(ui.parent(root), None);
    assert_eq!(ui.cursor(Some(stale)).next(), None);
}

#[test]
fn deletion_removes_every_branch_and_repairs_survivors() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let left = ui.create(Node::Box, ());
    let branch = ui.create(Node::Box, ());
    let right = ui.create(Node::Box, ());

    for child in [left, branch, right] {
        ui.append(root, child);
    }

    let mut deleted = vec![branch];
    for _ in 0..3 {
        let child = ui.create(Node::Box, ());
        ui.append(branch, child);
        deleted.push(child);

        for _ in 0..2 {
            let leaf = ui.create(Node::Text, ());
            ui.append(child, leaf);
            deleted.push(leaf);
        }
    }

    ui.delete(branch);

    for key in deleted {
        assert!(ui.node(key).is_none());
    }
    check(&ui, root, &[left, right]);

    ui.delete(left);
    check(&ui, root, &[right]);

    ui.delete(right);
    check(&ui, root, &[]);

    ui.delete(branch);
    ui.clear();
    assert!(ui.props(root).is_none());

    let fresh = ui.create(Node::Box, ());
    assert_eq!(*ui.node(fresh).unwrap(), Node::Box);
    check(&ui, fresh, &[]);
}

#[test]
fn properties_and_required_node_kind() {
    let mut ui = Ui::new();
    let key = ui.create(Node::Text, (Width::default(), Text("hello".into())));
    assert_eq!(*ui.node(key).unwrap(), Node::Text);
    assert_eq!(ui.prop::<Width>(key).unwrap().0, 10);

    ui.prop_mut::<Width>(key).unwrap().0 = 20;
    {
        let props = ui.props(key).unwrap();
        assert_eq!(props.get::<Width>().unwrap().0, 20);
        props.get_mut::<Text>().unwrap().0.push('!');
    }

    ui.set_props(key, (Width(30), Color(42)));
    assert_eq!(ui.prop::<Width>(key).unwrap().0, 30);
    assert_eq!(ui.prop::<Text>(key).unwrap().0, "hello!");
    assert_eq!(ui.prop::<Color>(key).unwrap().0, 42);

    assert_eq!(ui.remove_prop::<Width>(key), Some(Width(30)));
    assert!(ui.prop::<Width>(key).is_none());
    assert_eq!(*ui.node(key).unwrap(), Node::Text);

    let empty = ui.create(Node::Box, ());
    assert!(ui.prop::<Color>(empty).is_none());

    ui.delete(key);
    ui.set_props(key, (Width(1),));
    assert!(ui.props(key).is_none());
    assert!(ui.prop_mut::<Width>(key).is_none());
    assert!(ui.remove_prop::<Text>(key).is_none());
}
