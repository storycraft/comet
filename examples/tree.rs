use comet::{
    layout::{InlineItem, LayoutBoxKey, LayoutBoxTree, LayoutBoxTreeCx, LayoutTy},
    node::{DisplayInner, DisplayOuter, Node, NodeKey, UiTree},
};

fn main() {
    let mut ui = UiTree::new();
    let root = ui.create_div();
    let text1 = ui.create_text("sample text");
    let inner = ui.create_text("start");
    let div = ui.create_div();
    if let Some(Node::Div(div)) = ui.get_mut(div) {
        div.display = Some((DisplayOuter::Inline, DisplayInner::FlowRoot));
    }
    let div2 = ui.create_div();
    let inner2 = ui.create_text("end");
    let text2 = ui.create_text("1");
    ui.append_child(root, text1);
    ui.append_child(root, div);
    ui.append_child(div, inner);
    ui.append_child(root, div2);
    ui.append_child(div2, inner2);
    ui.append_child(root, text2);

    let mut layout_tree = LayoutBoxTree::new();
    let mut tree_cx = LayoutBoxTreeCx::new();
    tree_cx.build(&ui, root, &mut layout_tree);
    let box_root = layout_tree.root;

    layout_tree.compute_layout(
        &mut ui,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(100.0),
            height: taffy::AvailableSpace::Definite(100.0),
        },
    );
    print_box_tree(&layout_tree, box_root, 0);

    print(&ui, root, 0);
}

fn print_box_tree(layout_tree: &LayoutBoxTree, id: LayoutBoxKey, space: u32) {
    let Some(node) = layout_tree.boxes.get(id) else {
        return;
    };

    for _ in 0..space {
        print!(" ");
    }

    println!(
        "- span: {:?} ty: {:?} location: {:?} size: {:?}",
        node.span, node.ty, node.taffy_layout.location, node.taffy_layout.size
    );

    match node.ty {
        LayoutTy::Block => {}
        LayoutTy::Inline(inline_box_id) => {
            let inline_box = &layout_tree.inline_boxes[inline_box_id];
            for child_id in layout_tree.inlines.cursor(inline_box.item_start) {
                match layout_tree.inlines[child_id] {
                    InlineItem::Text { start, end } => {
                        for _ in 0..space {
                            print!(" ");
                        }
                        println!("    - text: {:?}", &layout_tree.texts[start..end]);
                    }
                    InlineItem::Box(child_box) => {
                        print_box_tree(layout_tree, child_box, space + 4);
                    }
                }
            }
        }
    }

    for child_id in layout_tree.boxes.cursor(layout_tree.boxes.first_child(id)) {
        print_box_tree(layout_tree, child_id, space + 4);
    }
}

fn print(tree: &UiTree, id: NodeKey, space: u32) {
    for _ in 0..space {
        print!(" ");
    }
    println!("- {:?}", tree.get(id));
    for child in tree.children(id) {
        print(tree, *child, space + 4);
    }
}
