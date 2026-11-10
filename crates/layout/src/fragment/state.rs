use super::FragmentKey;
use crate::taffy::style::Style;

/// Retained layout state of a box generating item.
pub(crate) struct NodeState {
    pub style: Style,
    /// Unrounded layout of the latest final layout pass.
    pub layout: taffy::Layout,
    pub cache: taffy::Cache,
    /// Whether retained text and descendant geometry match the final cache entry.
    pub final_layout_valid: bool,
    /// Baseline relative to the border box top, for atomic inlines and roots.
    pub baseline: f32,
    pub hoisted: Vec<taffy::NodeId>,
    /// Box fragment of this item. Its geometry is written in place.
    pub fragment: FragmentKey,
}

impl NodeState {
    pub fn new(style: Style, fragment: FragmentKey) -> Self {
        Self {
            style,
            layout: Default::default(),
            cache: Default::default(),
            final_layout_valid: false,
            baseline: 0.0,
            hoisted: Vec::new(),
            fragment,
        }
    }
}
