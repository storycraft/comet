pub enum NodeType {
    Inline,
    Block,
    Opaque,
}

pub struct Node {
    pub ty: NodeType,
    pub id: u32,
}

pub enum NodeAttr {
    Width(u32),
    Height(u32),
}

pub enum NodeCommand {
    Push(Node),
    Pop,
    Attr(NodeAttr),
}

pub struct LayoutContext {

}