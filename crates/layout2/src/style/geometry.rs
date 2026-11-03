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
            StyleUnit::Pw(v) => cx.parent_size.width * v,
            StyleUnit::Ph(v) => cx.parent_size.height * v,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, derive_more::Display)]
pub enum AutoValue<T> {
    #[default]
    Auto,
    Value(T),
}

impl<T> AutoValue<T> {
    pub fn value_default() -> Self
    where
        T: Default,
    {
        Self::Value(Default::default())
    }

    pub fn resolve(self, auto_value: T) -> T {
        match self {
            AutoValue::Auto => auto_value,
            AutoValue::Value(v) => v,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, derive_more::Display)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, derive_more::Display)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

    #[inline]
    pub const fn new_invidiual(top: T, right: T, bottom: T, left: T) -> Self {
        Self {
            left_top: Point::new(left, top),
            right_bottom: Point::new(right, bottom),
        }
    }

    #[inline]
    pub const fn new_axis(top_bottom: T, left_right: T) -> Self
    where
        T: Copy,
    {
        Self::new_invidiual(top_bottom, left_right, top_bottom, left_right)
    }
}

impl Rect<StyleUnit> {
    #[inline]
    pub const fn resolve(self, cx: &LayoutConstraint) -> Rect<f64> {
        Rect::new(self.left_top.resolve(cx), self.right_bottom.resolve(cx))
    }
}
