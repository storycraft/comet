pub(crate) struct NodeState {
    pub style: crate::taffy::style::Style,
    pub layout: taffy::Layout,
    pub cache: taffy::Cache,
    /// Whether retained text and descendant geometry match the final cache entry.
    pub final_layout_valid: bool,
    pub baseline: f32,
    pub hoisted: Vec<taffy::NodeId>,
}

impl NodeState {
    pub fn new(style: crate::taffy::style::Style) -> Self {
        Self {
            style,
            layout: Default::default(),
            cache: Default::default(),
            final_layout_valid: false,
            baseline: 0.0,
            hoisted: Vec::new(),
        }
    }
}
