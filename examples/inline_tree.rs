use comet::{
    layout::{
        cx::LayoutContext,
        inline::{
            builder::InlineTreeBuilder,
            tree::{InlineNodeKey, InlineNodeTy, InlineTree},
        },
        tree::{LayoutNode, LayoutNodeTy, LayoutTree},
    },
    style::div::{DisplayInner, DisplayOuter},
    ui::{Node, Ui, layout::UiLayoutBuilder},
};
use parley::FontContext;

fn main() {
    let mut ui = Ui::new();
    let text0 = ui.create_node(Node::Text("sample".to_string()), ());
    let text1 = ui.create_node(Node::Text("text".to_string()), ());
    let inner = ui.create_node(Node::Text("start".to_string()), ());
    let root = ui.create_node(Node::Div, (DisplayOuter::Inline,));
    let div1 = ui.create_node(Node::Div, (DisplayOuter::Inline, DisplayInner::FlowRoot));

    ui.append(root, text0);
    ui.append(root, div1);
    ui.append(root, text1);
    ui.append(div1, inner);

    let mut layout_tree = LayoutTree::new();
    let layout_root = layout_tree.create_root();

    let mut tree_builder = UiLayoutBuilder::new();
    tree_builder.build(&ui, root, &mut layout_tree, layout_root);

    let mut layout_cx = LayoutContext::new();
    layout_cx.layout(
        &mut FontContext::new(),
        &ui,
        &mut layout_tree,
        layout_root,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(256.0),
            height: taffy::AvailableSpace::Definite(256.0),
        },
    );

    let LayoutNode {
        ty: LayoutNodeTy::Inline(inline_node_key),
        ..
    } = layout_tree
        .nodes
        .get(layout_tree.nodes.first_child(layout_root).unwrap())
        .unwrap()
    else {
        return;
    };

    let mut inline_tree = InlineTree::new();
    let mut inline_builder = InlineTreeBuilder::new();
    let inline_node = inline_builder
        .build(&layout_tree, &mut inline_tree, *inline_node_key)
        .unwrap();

    print_inline_tree(&ui, &inline_tree, inline_node, 0);
}

fn print_inline_tree(ui: &Ui, tree: &InlineTree, id: InlineNodeKey, space: u32) {
    let Some(node) = tree.nodes.get(id) else {
        println!();
        return;
    };

    for _ in 0..space {
        print!(" ");
    }
    print!("- id: {id:?} layout: {:?}", node.layout);

    match node.ty {
        InlineNodeTy::Box(span) => {
            println!(" ty: Box span: {span:?}");
        }
        InlineNodeTy::Text { .. } => {
            println!(" ty: Text");
        }
        InlineNodeTy::LayoutNode(span) => {
            println!(" ty: Node span: {span:?}");
        }
    }

    for child_id in tree.nodes.cursor(tree.nodes.first_child(id)) {
        print_inline_tree(ui, tree, child_id, space + 4);
    }
}
