use comet::{
    layout::{BoxKey, BoxLayoutTree, BoxLayoutTreeCx, BoxNodeTy, InlineItem},
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

    let mut box_tree = BoxLayoutTree::new();
    let mut box_tree_cx = BoxLayoutTreeCx::new();
    box_tree_cx.build(&ui, root, &mut box_tree);
    let box_root = box_tree.root;

    box_tree.compute_layout(
        &mut ui,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(100.0),
            height: taffy::AvailableSpace::Definite(100.0),
        },
    );
    print_box_tree(&box_tree, box_root, 0);

    print(&ui, root, 0);
}

fn print_box_tree(box_tree: &BoxLayoutTree, id: BoxKey, space: u32) {
    let Some(node) = box_tree.map.get(id) else {
        return;
    };

    for _ in 0..space {
        print!(" ");
    }

    println!(
        "- span: {:?} ty: {:?} location: {:?} size: {:?}",
        node.span, node.ty, node.unrounded_layout.location, node.unrounded_layout.size
    );

    match node.ty {
        BoxNodeTy::Block(ref block) => {
            let mut child = block.first_child;
            while let Some(child_id) = child {
                print_box_tree(box_tree, child_id, space + 4);
                child = box_tree.map.get(child_id).and_then(|node| node.next_sibling);
            }
        }
        BoxNodeTy::Inline(ref inline) => {
            for child in &inline.children {
                match *child {
                    InlineItem::Text { start, end } => {
                        for _ in 0..space {
                            print!(" ");
                        }
                        println!("    - text: {:?}", &box_tree.texts[start..end]);
                    }
                    InlineItem::Box(child_box) => {
                        print_box_tree(box_tree, child_box, space + 4);
                    }
                }
            }
        }
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
