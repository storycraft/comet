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
