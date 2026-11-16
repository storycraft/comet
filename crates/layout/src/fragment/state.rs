use bitflags::bitflags;

use super::{FragmentKey, InlineLayout};
use crate::{input::LayoutInputKey, style::geometry::StyleUnit, taffy::style::Style};

bitflags! {
    /// What the layout of a box or of its subtree depends on, besides its
    /// own content. Set when box styles are resolved and from what custom
    /// container algorithms return, and on every ancestor box up to the root
    /// of the build.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
    pub struct NodeLayoutBits: u8 {
        /// A length resolves against the parent's size, such as a percentage.
        const DEPENDS_PARENT_SIZE = 1 << 0;
        /// A length uses `vw` or `vh`.
        const DEPENDS_VIEWPORT = 1 << 1;
    }
}

impl NodeLayoutBits {
    /// What a length of `unit` depends on.
    pub fn of_unit(unit: StyleUnit) -> Self {
        match unit {
            StyleUnit::Percent(_) => Self::DEPENDS_PARENT_SIZE,
            unit if unit.is_viewport_relative() => Self::DEPENDS_VIEWPORT,
            _ => Self::empty(),
        }
    }
}

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
    /// What the layout of this box and of its subtree depends on. Bits are
    /// only added, so they may outlive the styles that set them.
    pub bits: NodeLayoutBits,
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
            bits: NodeLayoutBits::empty(),
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
