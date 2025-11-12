use slotmap::new_key_type;

new_key_type! { pub struct NodeKey; }

#[derive(Debug)]
pub enum Node {
    Div,
    Text(String),
}

