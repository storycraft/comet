use crate::style::LayoutConstraint;

/// Relative, Absolute size unit
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StyleUnit {
    /// Pixel unit
    Px(f64),
    /// Percentage of parent width. 100% = 1.0
    Pw(f64),
    /// Percentage of parent height. 100% = 1.0
    Ph(f64),
    /// Em unit (relative to parent font size)
    Em(f64),
    /// Rem unit (relative to root font size)
    Rem(f64),
    /// Percentage of root width. 100vw = 1.0
    Vw(f64),
    /// Percentage of root height. 100vh = 1.0
    Vh(f64),
}

impl StyleUnit {
    pub const ZERO: StyleUnit = StyleUnit::Px(0.0);

    pub const fn resolve(self, cx: &LayoutConstraint) -> f64 {
        match self {
            StyleUnit::Px(v) => v,
            StyleUnit::Pw(v) => cx.percentage_resolution_size.width * v,
            StyleUnit::Ph(v) => cx.percentage_resolution_size.height * v,
            StyleUnit::Em(v) => v * cx.font_size,
            StyleUnit::Rem(v) => v * cx.root_font_size,
            StyleUnit::Vw(v) => v * cx.root_size.width,
            StyleUnit::Vh(v) => v * cx.root_size.height,
        }
    }
}

impl Default for StyleUnit {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum AutoValue<T> {
    Auto,
    Value(T),
}

impl<T> AutoValue<T> {
    pub fn resolve(self, auto_value: T) -> T {
        match self {
            AutoValue::Auto => auto_value,
            AutoValue::Value(v) => v,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
#[display("Size({width}, {height})")]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}

impl<T> Size<T> {
    #[inline]
    pub const fn new(width: T, height: T) -> Self {
        Self { width, height }
    }
}

impl Size<StyleUnit> {
    #[inline]
    pub const fn resolve(self, cx: &LayoutConstraint) -> Size<f64> {
        Size::new(self.width.resolve(cx), self.height.resolve(cx))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
#[display("Point({x}, {y})")]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

impl<T> Point<T> {
    #[inline]
    pub const fn new(x: T, y: T) -> Self {
        Self { x, y }
    }
}

impl Point<StyleUnit> {
    #[inline]
    pub const fn resolve(self, cx: &LayoutConstraint) -> Point<f64> {
        Point::new(self.x.resolve(cx), self.y.resolve(cx))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect<T> {
    pub left_top: Point<T>,
    pub right_bottom: Point<T>,
}

impl<T> Rect<T> {
    #[inline]
    pub const fn new(left_top: Point<T>, right_bottom: Point<T>) -> Self {
        Self {
            left_top,
            right_bottom,
        }
    }
}

impl Rect<StyleUnit> {
    #[inline]
    pub const fn resolve(self, cx: &LayoutConstraint) -> Rect<f64> {
        Rect::new(self.left_top.resolve(cx), self.right_bottom.resolve(cx))
    }
}
