pub(crate) struct NodeState {
    pub style: crate::taffy::style::Style,
    pub layout: taffy::Layout,
    pub cache: taffy::Cache,
    pub baseline: f32,
    pub hoisted: Vec<taffy::NodeId>,
}

impl NodeState {
    pub fn new(style: crate::taffy::style::Style) -> Self {
        Self {
            style,
            layout: Default::default(),
            cache: Default::default(),
            baseline: 0.0,
            hoisted: Vec::new(),
        }
    }
}
