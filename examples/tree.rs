use comet::{
    layout::{pass::LayoutPassCx, pre_pass::LayoutPrePassCx}, node::{LayoutTree, NodeKey}
};
use parley::{Alignment, AlignmentOptions};

fn main() {
    let mut layout_tree = LayoutTree::new();
    let root = layout_tree.create_div();
    let text1 = layout_tree.create_text("sample text");
    let inner = layout_tree.create_text("start");
    let div = layout_tree.create_div();
    let div2 = layout_tree.create_div();
    let inner2 = layout_tree.create_text("end");
    let text2 = layout_tree.create_text("1");
    layout_tree.append_child(root, text1);
    layout_tree.append_child(root, div);
    layout_tree.append_child(div, inner);
    layout_tree.append_child(root, div2);
    layout_tree.append_child(div2, inner2);
    layout_tree.append_child(root, text2);

    let mut pre_pass = LayoutPrePassCx::new();
    pre_pass.accept( &layout_tree, root);
    dbg!(&mut pre_pass);

    let mut pass = LayoutPassCx::new();
    pass.accept(&mut pre_pass);

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
    print(&layout_tree, root, 0);
}

fn print(tree: &LayoutTree, id: NodeKey, space: u32) {
    for _ in 0..space {
        print!(" ");
    }
    println!("- {:?}", tree.get(id));
    for child in tree.children(id) {
        print(tree, *child, space + 4);
    }
}
