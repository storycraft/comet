use taffy::{
    BlockContainerStyle, BlockItemStyle, BoxGenerationMode, BoxSizing, CoreStyle,
    LengthPercentageAuto, Overflow, Point, Rect, TextAlign,
};

use crate::{layout::DisplayInner, node::Div};

pub struct TaffyCoreStyle<'a>(&'a Div);

impl Default for TaffyCoreStyle<'static> {
    fn default() -> Self {
        Self(const { &Div::new() })
    }
}

impl CoreStyle for TaffyCoreStyle<'_> {
    type CustomIdent = String;

    fn box_generation_mode(&self) -> BoxGenerationMode {
        if self.0.display_outer.is_some() {
            BoxGenerationMode::Normal
        } else {
            BoxGenerationMode::None
        }
    }

    fn is_block(&self) -> bool {
        matches!(
            self.0.display_inner,
            DisplayInner::Flow | DisplayInner::FlowRoot
        )
    }

    fn is_compressible_replaced(&self) -> bool {
        false
    }

    fn box_sizing(&self) -> BoxSizing {
        self.0.box_sizing
    }

    fn overflow(&self) -> Point<Overflow> {
        self.0.overflow
    }

    fn scrollbar_width(&self) -> f32 {
        0.0
    }

    fn position(&self) -> taffy::Position {
        self.0.position
    }

    fn inset(&self) -> Rect<LengthPercentageAuto> {
        self.0.inset
    }

    fn size(&self) -> taffy::Size<taffy::Dimension> {
        self.0.size
    }

    fn min_size(&self) -> taffy::Size<taffy::Dimension> {
        self.0.min_size
    }

    fn max_size(&self) -> taffy::Size<taffy::Dimension> {
        self.0.max_size
    }

    fn aspect_ratio(&self) -> Option<f32> {
        self.0.aspect_ratio
    }

    fn margin(&self) -> taffy::Rect<taffy::LengthPercentageAuto> {
        self.0.margin
    }

    fn padding(&self) -> taffy::Rect<taffy::LengthPercentage> {
        self.0.padding
    }

    fn border(&self) -> taffy::Rect<taffy::LengthPercentage> {
        self.0.border
    }
}

impl BlockContainerStyle for TaffyCoreStyle<'_> {
    fn text_align(&self) -> TextAlign {
        TextAlign::Auto
    }
}

impl BlockItemStyle for TaffyCoreStyle<'_> {
    fn is_table(&self) -> bool {
        false
    }
}
