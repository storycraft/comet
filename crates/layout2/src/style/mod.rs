pub mod bx;
pub mod geometry;

use crate::style::geometry::{Point, Rect, Size, StyleUnit};

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutConstraint {
    pub root_size: Size<f64>,
    pub root_font_size: f64,

    pub available_size: Size<f64>,
    pub percentage_resolution_size: Size<f64>,
    pub font_size: f64,
}

impl LayoutConstraint {
    pub const fn new_root(root_size: Size<f64>, font_size: f64) -> Self {
        Self {
            root_size,
            root_font_size: font_size,
            available_size: root_size,
            percentage_resolution_size: root_size,
            font_size,
        }
    }

    pub const fn child(
        &self,
        available_size: Size<f64>,
        percentage_resolution_size: Size<f64>,
        font_size: f64,
    ) -> Self {
        Self {
            root_size: self.root_size,
            root_font_size: self.root_font_size,
            available_size,
            percentage_resolution_size,
            font_size,
        }
    }

    pub const fn resolve(&self, unit: StyleUnit) -> f64 {
        match unit {
            StyleUnit::Px(v) => v,
            StyleUnit::Pw(v) => self.percentage_resolution_size.width * v,
            StyleUnit::Ph(v) => self.percentage_resolution_size.height * v,
            StyleUnit::Em(v) => v * self.font_size,
            StyleUnit::Rem(v) => v * self.root_font_size,
            StyleUnit::Vw(v) => v * self.root_size.width,
            StyleUnit::Vh(v) => v * self.root_size.height,
        }
    }

    #[inline]
    pub const fn resolve_point(&self, point: Point<StyleUnit>) -> Point<f64> {
        Point::new(self.resolve(point.x), self.resolve(point.y))
    }

    #[inline]
    pub const fn resolve_size(&self, size: Size<StyleUnit>) -> Size<f64> {
        Size::new(self.resolve(size.width), self.resolve(size.height))
    }

    #[inline]
    pub const fn resolve_rect(&self, rect: Rect<StyleUnit>) -> Rect<f64> {
        Rect::new(
            self.resolve_point(rect.left_top),
            self.resolve_point(rect.right_bottom),
        )
    }
}
