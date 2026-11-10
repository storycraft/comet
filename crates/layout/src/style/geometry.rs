use crate::style::LayoutConstraint;

/// Relative, Absolute size unit
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StyleUnit {
    /// Pixel unit
    Px(f64),
    /// Percentage of the property's reference value. 100% = 1.0.
    Percent(f64),
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

    /// OLD
    /// Resolve a length with the percentage basis chosen by the caller.
    pub const fn resolve(self, cx: &LayoutConstraint, percentage_basis: f64) -> f64 {
        match self {
            StyleUnit::Px(v) => v,
            StyleUnit::Percent(v) => percentage_basis * v,
            StyleUnit::Em(v) => v * cx.font_size,
            StyleUnit::Rem(v) => v * cx.root_font_size,
            StyleUnit::Vw(v) => v * cx.root_size.width,
            StyleUnit::Vh(v) => v * cx.root_size.height,
        }
    }

    /// Resolve a length.
    pub const fn resolve2(self, cx: &UnitResolveContext) -> f64 {
        match self {
            StyleUnit::Px(v) => v,
            StyleUnit::Percent(v) => cx.percentage_basis * v,
            StyleUnit::Em(v) => v * cx.font_size,
            StyleUnit::Rem(v) => v * cx.root_font_size,
            StyleUnit::Vw(v) => v * cx.viewport.width,
            StyleUnit::Vh(v) => v * cx.viewport.height,
        }
    }
}

impl Default for StyleUnit {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnitResolveContext {
    pub percentage_basis: f64,
    pub root_font_size: f64,
    pub font_size: f64,
    pub viewport: Size<f64>,
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
    pub const fn resolve(self, cx: &LayoutConstraint, percentage_basis: Size<f64>) -> Size<f64> {
        Size::new(
            self.width.resolve(cx, percentage_basis.width),
            self.height.resolve(cx, percentage_basis.height),
        )
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
    pub const fn resolve(self, cx: &LayoutConstraint, percentage_basis: Point<f64>) -> Point<f64> {
        Point::new(
            self.x.resolve(cx, percentage_basis.x),
            self.y.resolve(cx, percentage_basis.y),
        )
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

    #[inline]
    pub const fn new_all(all: T) -> Self
    where
        T: Copy,
    {
        Self::new_axis(all, all)
    }
}

impl Rect<StyleUnit> {
    #[inline]
    pub const fn resolve(self, cx: &LayoutConstraint, percentage_basis: Rect<f64>) -> Rect<f64> {
        Rect::new(
            self.left_top.resolve(cx, percentage_basis.left_top),
            self.right_bottom.resolve(cx, percentage_basis.right_bottom),
        )
    }
}
