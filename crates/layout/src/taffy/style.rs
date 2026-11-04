use comet_tree::{node::NodeKey, ui::Ui};
use taffy::style::{BlockContainerStyle, BlockItemStyle, CoreStyle, OofItemStyle};

use super::conv;
use crate::style::{
    LayoutConstraint,
    bx::{self, DisplayInner},
};

/// Box style adapter shared by layout algorithms.
///
/// Resolves constraint-dependent units during construction. Percentages remain
/// symbolic so the algorithm can choose their basis. Algorithm selection is independent
/// of this adapter, allowing custom containers to reuse the common box styles.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    inner: DisplayInner,
    position: taffy::Position,
    box_sizing: taffy::BoxSizing,
    overflow: taffy::Point<taffy::Overflow>,
    inset: taffy::Rect<taffy::LengthPercentageAuto>,
    size: taffy::Size<taffy::Dimension>,
    min_size: taffy::Size<taffy::LengthPercentageAuto>,
    max_size: taffy::Size<taffy::LengthPercentageAuto>,
    aspect_ratio: Option<f32>,
    margin: taffy::Rect<taffy::LengthPercentageAuto>,
    padding: taffy::Rect<taffy::LengthPercentage>,
    border: taffy::Rect<taffy::LengthPercentage>,
}

/// Box properties never inherit. Anonymous boxes use the declared defaults.
pub fn resolve(ui: &Ui, node: Option<NodeKey>, cx: &LayoutConstraint) -> Style {
    macro_rules! value {
        ($prop:ident) => {
            node.and_then(|node| ui.prop::<bx::$prop>(node))
                .map(|v| v.0)
                .unwrap_or_else(|| bx::$prop::default().0)
        };
    }

    Style {
        inner: node
            .and_then(|node| ui.prop::<DisplayInner>(node))
            .as_deref()
            .copied()
            .unwrap_or_default(),
        position: value!(Position),
        box_sizing: value!(BoxSizing),
        overflow: conv::to_point(value!(Overflow), |v| v),
        inset: conv::to_rect(value!(Inset), |v| conv::length_auto(v, cx)),
        size: conv::to_size(value!(Size), |v| conv::dimension(v, cx)),
        min_size: conv::to_size(value!(MinSize), |v| conv::length_auto(v, cx)),
        max_size: conv::to_size(value!(MaxSize), |v| conv::length_auto(v, cx)),
        aspect_ratio: node
            .and_then(|node| ui.prop::<bx::AspectRatio>(node))
            .map(|v| v.0),
        margin: conv::to_rect(value!(Margin), |v| conv::length_auto(v, cx)),
        padding: conv::to_rect(value!(Padding), |v| conv::length(v, cx)),
        border: conv::to_rect(value!(Border), |v| conv::length(v, cx)),
    }
}

impl CoreStyle for Style {
    type CustomIdent = String;

    fn is_block(&self) -> bool {
        matches!(self.inner, DisplayInner::Flow | DisplayInner::Content)
    }

    fn is_replaced(&self) -> bool {
        self.inner == DisplayInner::Content
    }

    fn is_compressible_replaced(&self) -> bool {
        self.is_replaced()
    }

    fn position(&self) -> taffy::Position {
        self.position
    }

    fn box_sizing(&self) -> taffy::BoxSizing {
        self.box_sizing
    }

    fn overflow(&self) -> taffy::Point<taffy::Overflow> {
        self.overflow
    }

    fn inset(&self) -> taffy::Rect<taffy::LengthPercentageAuto> {
        self.inset
    }

    fn size(&self) -> taffy::Size<taffy::Dimension> {
        self.size
    }

    fn min_size(&self) -> taffy::Size<taffy::LengthPercentageAuto> {
        self.min_size
    }

    fn max_size(&self) -> taffy::Size<taffy::LengthPercentageAuto> {
        self.max_size
    }

    fn aspect_ratio(&self) -> Option<f32> {
        self.aspect_ratio
    }

    fn margin(&self) -> taffy::Rect<taffy::LengthPercentageAuto> {
        self.margin
    }

    fn padding(&self) -> taffy::Rect<taffy::LengthPercentage> {
        self.padding
    }

    fn border(&self) -> taffy::Rect<taffy::LengthPercentage> {
        self.border
    }
}

impl BlockContainerStyle for Style {}

impl BlockItemStyle for Style {}

impl OofItemStyle for Style {}
