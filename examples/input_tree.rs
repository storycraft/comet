use comet::{
    layout::input::{InputNode, InputNodeKey, LayoutInputTree, builder::LayoutInputTreeContext},
    style::div::{DisplayInner, DisplayOuter},
    ui::{Node, Ui},
};

fn main() {
    let mut ui = Ui::new();

    let root = ui.create_node(Node::Div, ());
    let text0 = ui.create_node(Node::Text("sample".to_string()), ());
    let text1 = ui.create_node(Node::Text("text".to_string()), ());
    let inner = ui.create_node(Node::Text("start".to_string()), ());
    let div = ui.create_node(Node::Div, (DisplayOuter::Inline,));
    let div1 = ui.create_node(Node::Div, (DisplayOuter::Inline, DisplayInner::FlowRoot));

    let div2 = ui.create_node(Node::Div, ());

    let inner2 = ui.create_node(Node::Text("end".to_string()), ());
    let text2 = ui.create_node(Node::Text("1".to_string()), ());
    ui.append(div, text0);
    ui.append(div, div1);
    ui.append(div, text1);
    ui.append(root, div);
    ui.append(div1, inner);
    ui.append(root, div2);
    ui.append(div2, inner2);
    ui.append(root, text2);

    let mut input_tree = LayoutInputTree::new();
    let input_root = input_tree.create_root();

    let mut input_tree_cx = LayoutInputTreeContext::new();
    input_tree_cx.build_full(&ui, root, &mut input_tree, input_root);

    print_input_box_tree(&ui, &input_tree, input_root, 0);

    dbg!(input_tree_cx.update(&ui, &mut input_tree, text2));
    print_input_box_tree(&ui, &input_tree, input_root, 0);
}

fn print_input_box_tree(ui: &Ui, input_tree: &LayoutInputTree, id: InputNodeKey, space: u32) {
    for _ in 0..space {
        print!(" ");
    }
    print!("- id: {id:?}");

    let Some(node) = input_tree.nodes.get(id) else {
        println!();
        return;
    };
    match node {
        InputNode::Block(span) => {
            println!(" ty: Block span: {span:?}");
        }
        InputNode::Inline(inline_node) => {
            println!(" ty: Inline text: {}", inline_node.texts);
            for _ in 0..(space + 4) {
                print!(" ");
            }

            for child_id in input_tree.inlines.cursor(inline_node.inline_start) {
                print!("{:?}, ", input_tree.inlines[child_id]);
            }
            println!();
        }
    }

    for child_id in input_tree.nodes.cursor(input_tree.nodes.first_child(id)) {
        print_input_box_tree(ui, input_tree, child_id, space + 4);
    }
}
