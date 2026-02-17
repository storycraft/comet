use comet_div::prelude::*;
use comet_layout::{
    style::*,
    tree::{LayoutNodeKey, LayoutNodeTy, LayoutTree, builder::UiLayoutBuilder},
};

fn main() {
    let mut ui = Ui::new();

    let root = ui.create(Node::Div, ());
    let text0 = ui.create(Node::Text("sam\nple".to_string()), ());
    let text1 = ui.create(Node::Text("text".to_string()), ());
    let inner = ui.create(Node::Text("start".to_string()), ());
    let div = ui.create(Node::Div, (DisplayOuter::Inline,));
    let div1 = ui.create(Node::Div, (DisplayOuter::Inline, DisplayInner::FlowRoot));

    let div2 = ui.create(Node::Div, ());

    let inner2 = ui.create(Node::Text("end".to_string()), ());
    let text2 = ui.create(Node::Text("1".to_string()), ());
    ui.append(div, text0);
    ui.append(div, div1);
    ui.append(div, text1);
    ui.append(root, div);
    ui.append(div1, inner);
    ui.append(root, div2);
    ui.append(div2, inner2);
    ui.append(root, text2);

    let mut layout_tree = LayoutTree::new();
    let input_root = layout_tree.create_root();

    let mut layout_builder = UiLayoutBuilder::new();
    layout_builder.build(&ui, root, &mut layout_tree, input_root);

    print_input_box_tree(&ui, &layout_tree, input_root, 0);

    dbg!(layout_builder.update(&ui, &mut layout_tree, text2));
    print_input_box_tree(&ui, &layout_tree, input_root, 0);
}

fn print_input_box_tree(ui: &Ui, tree: &LayoutTree, id: LayoutNodeKey, space: u32) {
    for _ in 0..space {
        print!(" ");
    }
    print!("- id: {id:?}");

    let Some(node) = tree.nodes.get(id) else {
        println!();
        return;
    };
    match node.ty {
        LayoutNodeTy::Block(span) => {
            println!(" ty: Block span: {span:?}");
        }
        LayoutNodeTy::Inline(inline_node_key) => {
            let Some(inline_node) = tree.inline_nodes.get(inline_node_key) else {
                return;
            };

            println!(" ty: Inline text: {:?}", inline_node.texts);
            for _ in 0..(space + 4) {
                print!(" ");
            }

            for child_id in tree.inlines.cursor(inline_node.inline_start) {
                print!("{:?}, ", tree.inlines[child_id]);
            }
            println!();
        }
    }

    for child_id in tree.nodes.cursor(tree.nodes.first_child(id)) {
        print_input_box_tree(ui, tree, child_id, space + 4);
    }
}
