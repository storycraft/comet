//! Geometry conversion, separate from the engine-independent geometry types.
use crate::style::{LayoutConstraint, geometry as g};

pub fn to_size<T, U>(v: g::Size<T>, mut f: impl FnMut(T) -> U) -> taffy::Size<U> {
    taffy::Size {
        width: f(v.width),
        height: f(v.height),
    }
}

pub fn from_size<T, U>(v: taffy::Size<T>, mut f: impl FnMut(T) -> U) -> g::Size<U> {
    g::Size::new(f(v.width), f(v.height))
}

pub fn to_point<T, U>(v: g::Point<T>, mut f: impl FnMut(T) -> U) -> taffy::Point<U> {
    taffy::Point {
        x: f(v.x),
        y: f(v.y),
    }
}

pub fn from_point<T, U>(v: taffy::Point<T>, mut f: impl FnMut(T) -> U) -> g::Point<U> {
    g::Point::new(f(v.x), f(v.y))
}

pub fn to_rect<T, U>(v: g::Rect<T>, mut f: impl FnMut(T) -> U) -> taffy::Rect<U> {
    taffy::Rect {
        left: f(v.left_top.x),
        top: f(v.left_top.y),
        right: f(v.right_bottom.x),
        bottom: f(v.right_bottom.y),
    }
}

pub fn from_rect<T, U>(v: taffy::Rect<T>, mut f: impl FnMut(T) -> U) -> g::Rect<U> {
    g::Rect::new_invidiual(f(v.top), f(v.right), f(v.bottom), f(v.left))
}

/// Percentages remain symbolic; their basis is selected by Taffy's CSS rules.
pub fn length(v: g::StyleUnit, cx: &LayoutConstraint) -> taffy::LengthPercentage {
    match v {
        g::StyleUnit::Percent(v) => taffy::LengthPercentage::percent(v as f32),
        v => taffy::LengthPercentage::length(v.resolve(&cx.units, 0.0) as f32),
    }
}

pub fn length_auto(
    v: g::AutoValue<g::StyleUnit>,
    cx: &LayoutConstraint,
) -> taffy::LengthPercentageAuto {
    match v {
        g::AutoValue::Auto => taffy::LengthPercentageAuto::auto(),
        g::AutoValue::Value(v) => length(v, cx).into(),
    }
}

pub fn dimension(v: g::AutoValue<g::StyleUnit>, cx: &LayoutConstraint) -> taffy::Dimension {
    length_auto(v, cx).into()
}
