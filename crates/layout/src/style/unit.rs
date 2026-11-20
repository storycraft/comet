//! Short constructors for style values.
//!
//! Units take the numbers CSS uses, so `percent(50)` is `50%` and `vh(100)`
//! is the full viewport height, unlike the fractions of [`StyleUnit`]
//! (`StyleUnit::Percent(0.5)`, `StyleUnit::Vh(1.0)`).
//!
//! Shapes convert each side into the expected value type, so units fill
//! [`AutoValue`] sides as well:
//!
//! ```
//! use comet_layout::style::{
//!     geometry::{AutoValue, Rect, Size, StyleUnit},
//!     unit::*,
//! };
//!
//! let padding: Rect<StyleUnit> = axis(px(8), px(16));
//! let min_size: Size<AutoValue<StyleUnit>> = size(auto(), vh(100));
//! assert_eq!(min_size.height, AutoValue::Value(StyleUnit::Vh(1.0)));
//! ```

use crate::style::geometry::{AutoValue, Point, Rect, Size, StyleUnit};

/// `v` pixels.
pub fn px(v: impl Into<f64>) -> StyleUnit {
    StyleUnit::Px(v.into())
}

/// `v` percent of the reference value: `percent(50)` is `50%`.
pub fn percent(v: impl Into<f64>) -> StyleUnit {
    StyleUnit::Percent(v.into() / 100.0)
}

/// `v` times the font size.
pub fn em(v: impl Into<f64>) -> StyleUnit {
    StyleUnit::Em(v.into())
}

/// `v` times the root font size.
pub fn rem(v: impl Into<f64>) -> StyleUnit {
    StyleUnit::Rem(v.into())
}

/// `v` percent of the viewport width: `vw(100)` is the full width.
pub fn vw(v: impl Into<f64>) -> StyleUnit {
    StyleUnit::Vw(v.into() / 100.0)
}

/// `v` percent of the viewport height: `vh(100)` is the full height.
pub fn vh(v: impl Into<f64>) -> StyleUnit {
    StyleUnit::Vh(v.into() / 100.0)
}

/// An automatic length.
pub const fn auto() -> AutoValue<StyleUnit> {
    AutoValue::Auto
}

/// A size of `width` and `height`.
pub fn size<T>(width: impl Into<T>, height: impl Into<T>) -> Size<T> {
    Size::new(width.into(), height.into())
}

/// A point at `x` and `y`.
pub fn point<T>(x: impl Into<T>, y: impl Into<T>) -> Point<T> {
    Point::new(x.into(), y.into())
}

/// The same value on every side.
pub fn all<T: Copy>(v: impl Into<T>) -> Rect<T> {
    Rect::new_all(v.into())
}

/// `vertical` on the top and bottom, `horizontal` on the left and right.
pub fn axis<T: Copy>(vertical: impl Into<T>, horizontal: impl Into<T>) -> Rect<T> {
    Rect::new_axis(vertical.into(), horizontal.into())
}

/// Each side, clockwise from the top.
pub fn sides<T>(
    top: impl Into<T>,
    right: impl Into<T>,
    bottom: impl Into<T>,
    left: impl Into<T>,
) -> Rect<T> {
    Rect::new_invidiual(top.into(), right.into(), bottom.into(), left.into())
}
