use taffy::{
    BlockContainerStyle, BlockItemStyle, BoxGenerationMode, BoxSizing, CoreStyle,
    LengthPercentageAuto, Overflow, Point, Position, Rect, TextAlign,
};

use crate::{
    style::{StyleProp, div::{
        AspectRatio, BorderWidth, DisplayInner, Inset, Margin, MaxSize, MinSize, Overflow1,
        Padding, Size1,
    }},
    ui::Props,
};

/// Taffy [`CoreStyle`], [`BlockContainerStyle`], [`BlockItemStyle`] wrapper
pub struct TaffyCoreStyle<'a>(pub Option<Props<'a>>);

impl<'a> TaffyCoreStyle<'a> {
    #[inline]
    fn get_cloned<T: StyleProp + Clone>(&self) -> Option<T> {
        Some(Clone::clone(&*self.0.as_ref()?.get::<T>()?))
    }
}

impl Default for TaffyCoreStyle<'_> {
    fn default() -> Self {
        Self(None)
    }
}

impl CoreStyle for TaffyCoreStyle<'_> {
    type CustomIdent = String;

    fn box_generation_mode(&self) -> BoxGenerationMode {
        BoxGenerationMode::Normal
    }

    fn is_block(&self) -> bool {
        matches!(
            self.get_cloned::<DisplayInner>().unwrap_or_default(),
            DisplayInner::Flow | DisplayInner::FlowRoot
        )
    }

    fn is_compressible_replaced(&self) -> bool {
        false
    }

    fn box_sizing(&self) -> BoxSizing {
        self.get_cloned::<BoxSizing>().unwrap_or_default()
    }

    fn overflow(&self) -> Point<Overflow> {
        self.get_cloned::<Overflow1>().unwrap_or_default().0
    }

    fn scrollbar_width(&self) -> f32 {
        0.0
    }

    fn position(&self) -> Position {
        self.get_cloned::<Position>().unwrap_or_default()
    }

    fn inset(&self) -> Rect<LengthPercentageAuto> {
        self.get_cloned::<Inset>().unwrap_or_default().0
    }

    fn size(&self) -> taffy::Size<taffy::Dimension> {
        self.get_cloned::<Size1>().unwrap_or_default().0
    }

    fn min_size(&self) -> taffy::Size<taffy::Dimension> {
        self.get_cloned::<MinSize>().unwrap_or_default().0
    }

    fn max_size(&self) -> taffy::Size<taffy::Dimension> {
        self.get_cloned::<MaxSize>().unwrap_or_default().0
    }

    fn aspect_ratio(&self) -> Option<f32> {
        self.get_cloned::<AspectRatio>().map(|v| v.0)
    }

    fn margin(&self) -> taffy::Rect<taffy::LengthPercentageAuto> {
        self.get_cloned::<Margin>().unwrap_or_default().0
    }

    fn padding(&self) -> taffy::Rect<taffy::LengthPercentage> {
        self.get_cloned::<Padding>().unwrap_or_default().0
    }

    fn border(&self) -> taffy::Rect<taffy::LengthPercentage> {
        let width = self.get_cloned::<BorderWidth>().unwrap_or_default().0;
        taffy::Rect {
            left: width,
            right: width,
            top: width,
            bottom: width,
        }
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
