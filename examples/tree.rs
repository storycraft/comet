use comet::{
    layout::{
        pass::LayoutPassCx,
        pass2::{BoxKey, BoxLayoutTree, BoxLayoutTreeCx, TreeNode},
        pre_pass::LayoutPrePassCx,
    },
    node::{DisplayInner, DisplayOuter, Node, NodeKey, UiTree},
};
use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext};

fn main() {
    let mut font_cx = FontContext::new();
    let mut layout_cx = LayoutContext::new();

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

    let mut pre_pass = LayoutPrePassCx::new();
    pre_pass.accept(&ui, root);
    dbg!(&mut pre_pass);

    let mut pass = LayoutPassCx::new();
    pass.accept(&mut pre_pass, &mut font_cx, &mut layout_cx);

    let mut box_tree = BoxLayoutTree::new();
    let mut box_tree_cx = BoxLayoutTreeCx::new();
    let root_box = box_tree_cx.build(&mut ui, root, &mut box_tree);
    print_box_tree(&mut box_tree, root_box, 0);

    for block in &mut pass.blocks {
        println!("block");
        block.layout.break_all_lines(None);
        block
            .layout
            .align(None, Alignment::Start, AlignmentOptions::default());
        dbg!(&block.text);
        dbg!(block.layout.width(), block.layout.height());
        for line in block.layout.lines() {
            println!("items");
            for items in line.items() {
                match items {
                    parley::PositionedLayoutItem::GlyphRun(glyph_run) => {
                        let runs = glyph_run.glyphs().collect::<Vec<_>>();
                        dbg!(glyph_run.style(), runs);
                    }
                    parley::PositionedLayoutItem::InlineBox(positioned_inline_box) => {
                        dbg!(positioned_inline_box);
                    }
                }
            }
        }
    }
    print(&ui, root, 0);
}

fn print_box_tree(box_tree: &BoxLayoutTree, id: BoxKey, space: u32) {
    for _ in 0..space {
        print!(" ");
    }

    let node = box_tree.map.get(id);
    println!("- {:?}", node);

    if let Some(TreeNode::Box(block)) = node {
        for child in &block.children {
            print_box_tree(box_tree, *child, space + 4);
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
