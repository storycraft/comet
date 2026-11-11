use super::{FragmentKey, InlineLayout};
use crate::{input::LayoutInputKey, taffy::style::Style};

/// How a box lays out its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Children are block level boxes laid out by Taffy's block algorithm.
    Block,
    /// Inline content without block boxes, measured as one Parley layout.
    Inline,
    /// Inline content interrupted by block boxes. It is laid out as a block
    /// container of anonymous blocks and the block boxes.
    MixedInline,
    /// Children are laid out by a custom container algorithm.
    Container,
    /// No children are laid out.
    Leaf,
    /// Anonymous block holding one run of inline content. The run starts after
    /// `after`, or at the start of the owner's inline content.
    Run { after: Option<LayoutInputKey> },
}

/// Retained layout state of a box or anonymous block fragment.
pub(crate) struct NodeState {
    /// Fragment owning this state, set when the fragment is inserted.
    pub fragment: FragmentKey,
    pub mode: Mode,
    pub style: Style,
    /// Unrounded layout of the latest final layout pass.
    pub layout: taffy::Layout,
    pub cache: taffy::Cache,
    /// Whether retained text and descendant geometry match the final cache entry.
    pub final_layout_valid: bool,
    /// Baseline relative to the border box top, for atomic inlines and roots.
    pub baseline: f32,
    pub hoisted: Vec<taffy::NodeId>,
    /// Shaped text of a fragment owning lines: the box of an inline
    /// formatting context, or an anonymous block.
    pub inline: Option<Box<InlineLayout>>,
}

impl NodeState {
    pub fn new(mode: Mode, style: Style) -> Self {
        Self {
            fragment: FragmentKey::default(),
            mode,
            style,
            layout: Default::default(),
            cache: Default::default(),
            final_layout_valid: false,
            baseline: 0.0,
            hoisted: Vec::new(),
            inline: None,
        }
    }

    /// Drop cached layouts, keeping the retained geometry.
    pub fn invalidate(&mut self) {
        self.cache.clear();
        self.final_layout_valid = false;
    }
}
