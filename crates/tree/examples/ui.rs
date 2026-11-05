use comet_tree::node::Text;
use comet_tree::prelude::*;

fn main() {
    let mut ui = Ui::new();

    let root = ui.create(Node::Box, ());
    let heading = ui.create(Node::Text, (Text("Welcome to Comet".into()),));
    let content = ui.create(Node::Box, ());
    let introduction = ui.create(Node::Text, (Text("A tree of boxes & text.".into()),));
    let details = ui.create(Node::Box, ());
    let description = ui.create(Node::Text, (Text("<text> holds text content.".into()),));
    let footer = ui.create(Node::Text, (Text("Thanks for visiting!".into()),));

    ui.append(root, heading);
    ui.append(root, content);
    ui.append(root, footer);

    ui.append(content, introduction);
    ui.append(content, details);
    ui.append(details, description);

    print_tree(&ui, root, 0);
}

fn print_tree(ui: &Ui, key: NodeKey, depth: usize) {
    let indent = "  ".repeat(depth);

    match ui.kind(key).expect("node exists in the example tree") {
        Node::Box => {
            println!("{indent}<box>");

            for child in ui.children(key) {
                print_tree(ui, child, depth + 1);
            }

            println!("{indent}</box>");
        }
        Node::Text => {
            let text = ui.prop::<Text>(key).expect("text element has content");
            let escaped = text
                .0
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");

            println!("{indent}{escaped}");
        }
    }
}
