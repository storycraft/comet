use comet_layout::{
    style::bx::DisplayInner,
    tree::{
        ChildLayoutMode, LayoutItem, LayoutItemLink, LayoutItemTy, LayoutTree,
        builder::LayoutTreeBuilder,
    },
};
use comet_tree::{node::Node, ui::Ui};

#[test]
fn scoped_updates_remove_obsolete_mappings_and_preserve_other_subtrees() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let content = ui.create(Node::Box, ());
    let text = ui.create(Node::Text, ());
    let sibling = ui.create(Node::Text, ());
    ui.append(root, content);
    ui.append(content, text);
    ui.append(root, sibling);

    let mut tree = LayoutTree::new();
    let mut builder = LayoutTreeBuilder::new();
    let root_key = builder.build(&ui, root, &mut tree, None).unwrap();
    let content_key = tree.item_for_node(content).unwrap();
    let text_key = tree.item_for_node(text).unwrap();
    let sibling_key = tree.item_for_node(sibling).unwrap();
    assert_eq!(tree.item_for_node(root), Some(root_key));
    assert_eq!(tree.items[text_key].ty, LayoutItemTy::Text(text));

    ui.set_props(content, (DisplayInner::Content,));
    assert_eq!(
        builder.build(&ui, content, &mut tree, Some(content_key)),
        Some(content_key)
    );
    assert_eq!(tree.item_for_node(content), Some(content_key));
    assert_eq!(tree.item_for_node(text), None);
    assert_eq!(tree.item_for_node(sibling), Some(sibling_key));

    ui.set_props(content, (DisplayInner::Flow,));
    builder
        .build(&ui, content, &mut tree, Some(content_key))
        .unwrap();
    assert_ne!(tree.item_for_node(text).unwrap(), text_key);
    assert_eq!(tree.item_for_node(sibling), Some(sibling_key));

    ui.delete(content);
    assert_eq!(
        builder.build(&ui, content, &mut tree, Some(content_key)),
        None
    );
    assert_eq!(tree.item_for_node(content), None);
    assert_eq!(tree.item_for_node(text), None);
    assert_eq!(tree.item_for_node(sibling), Some(sibling_key));

    tree.clear();

    for node in [root, content, text, sibling] {
        assert_eq!(tree.item_for_node(node), None);
    }

    let key = builder.build(&ui, sibling, &mut tree, None).unwrap();
    assert_eq!(tree.item_for_node(sibling), Some(key));
    assert_eq!(tree.items[key].ty, LayoutItemTy::Text(sibling));
}

#[test]
fn public_item_operations_maintain_source_lookups() {
    let mut ui = Ui::new();
    let root = ui.create(Node::Box, ());
    let child = ui.create(Node::Text, ());
    let absent = ui.create(Node::Box, ());
    let mut tree = LayoutTree::new();
    let root_key = tree.add_item(LayoutItem {
        ty: LayoutItemTy::Block(root),
        child_layout: ChildLayoutMode::Inline,
        link: LayoutItemLink::default(),
    });
    let child_key = tree
        .append_item(
            root_key,
            LayoutItem {
                ty: LayoutItemTy::Text(child),
                child_layout: ChildLayoutMode::None,
                link: LayoutItemLink::default(),
            },
        )
        .unwrap();
    assert_eq!(tree.item_for_node(root), Some(root_key));
    assert_eq!(tree.item_for_node(child), Some(child_key));
    assert_eq!(tree.item_for_node(absent), None);

    tree.delete_item(root_key).unwrap();
    assert_eq!(tree.item_for_node(root), None);
    assert_eq!(tree.item_for_node(child), None);
    assert_eq!(tree.delete_item(root_key), None);
}
