pub(crate) struct NodeState {
    pub style: taffy::Style,
    pub layout: taffy::Layout,
    pub cache: taffy::Cache,
    pub baseline: f32,
    pub hoisted: Vec<taffy::NodeId>,
}

impl NodeState {
    pub fn new(style: taffy::Style) -> Self {
        Self {
            style,
            layout: Default::default(),
            cache: Default::default(),
            baseline: 0.0,
            hoisted: Vec::new(),
        }
    }
}
