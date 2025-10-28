use slotmap::{HopSlotMap, new_key_type};

#[derive(Debug, PartialEq)]
pub enum Node {
    Div(Div),
    Text(String),
}

#[derive(Debug, PartialEq)]
pub struct Div {
    pub display: Option<(DisplayOuter, DisplayInner)>,
    children: Vec<NodeKey>,
}

impl Div {
    const fn new() -> Self {
        Self {
            display: Some((DisplayOuter::Block, DisplayInner::Flow)),
            children: Vec::new(),
        }
    }
}

impl Default for Div {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayOuter {
    #[default]
    Block,
    Inline,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayInner {
    #[default]
    Flow,
    FlowRoot,
    Flex,
    Grid,
}

new_key_type! { pub struct NodeKey; }

struct NodeItem {
    parent: Option<NodeKey>,
    node: Node,
}

pub struct UiTree {
    map: HopSlotMap<NodeKey, NodeItem>,
}

impl Default for UiTree {
    fn default() -> Self {
        Self::new()
    }
}

impl UiTree {
    pub fn new() -> Self {
        Self {
            map: HopSlotMap::with_key(),
        }
    }

    /// Create a new Text node
    pub fn create_text(&mut self, text: impl Into<String>) -> NodeKey {
        self.map.insert(NodeItem {
            parent: None,
            node: Node::Text(text.into()),
        })
    }

    /// Create a new [`Div`] node
    pub fn create_div(&mut self) -> NodeKey {
        self.map.insert(NodeItem {
            parent: None,
            node: Node::Div(Div::new()),
        })
    }

    pub fn get(&self, key: NodeKey) -> Option<&Node> {
        self.map.get(key).map(|item| &item.node)
    }

    pub fn get_mut(&mut self, key: NodeKey) -> Option<&mut Node> {
        self.map.get_mut(key).map(|item| &mut item.node)
    }

    /// Get parent of the node
    pub fn parent(&self, key: NodeKey) -> Option<NodeKey> {
        self.map.get(key)?.parent
    }

    /// Get children of the node
    pub fn children(&self, key: NodeKey) -> &[NodeKey] {
        let Some(item) = self.map.get(key) else {
            return &[];
        };

        match item.node {
            Node::Div(ref div) => &div.children,
            _ => &[],
        }
    }

    /// Remove child from its parent and return parent node
    pub fn remove_child(&mut self, child: NodeKey) -> Option<NodeKey> {
        let child_item = self.map.get_mut(child)?;
        let parent = child_item.parent.take()?;
        let Some(NodeItem {
            node: Node::Div(parent_div),
            ..
        }) = self.map.get_mut(parent)
        else {
            return None;
        };

        parent_div.children.retain(|key| *key == child);
        Some(parent)
    }

    /// Append node as a child
    pub fn append_child(&mut self, parent: NodeKey, child: NodeKey) -> Option<Option<NodeKey>> {
        let last_parent = self.remove_child(child);
        let Some(NodeItem {
            node: Node::Div(parent_div),
            ..
        }) = self.map.get_mut(parent)
        else {
            return None;
        };

        parent_div.children.push(child);
        Some(last_parent)
    }

    /// Delete a node from the tree
    pub fn delete(&mut self, key: NodeKey) -> bool {
        self.remove_child(key);
        self.map.remove(key).is_some()
    }
}
