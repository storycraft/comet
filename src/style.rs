pub mod div;
pub mod text;

use kurbo::Size;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum PropLevel {
    Paint = 0,
    Layout = 1,
}

pub trait StyleProp: Sized + Send + Sync + 'static {
    const LEVEL: PropLevel;
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutStyleCx {
    pub root_size: Size,
    pub root_font_size: f64,
    pub parent_size: Size,
    pub parent_font_size: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Relative, Absolute size unit
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

    #[inline]
    pub fn resolve(self, cx: &LayoutStyleCx) -> f64 {
        match self {
            Self::Px(v) => v,
            Self::Pw(v) => v * cx.parent_size.width,
            Self::Ph(v) => v * cx.parent_size.height,
            Self::Em(v) => v * cx.parent_font_size,
            Self::Rem(v) => v * cx.root_font_size,
            Self::Vw(v) => v * cx.root_size.width,
            Self::Vh(v) => v * cx.root_size.height,
        }
    }
}

impl Default for StyleUnit {
    fn default() -> Self {
        Self::ZERO
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StylePoint {
    pub x: StyleUnit,
    pub y: StyleUnit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleSize {
    pub width: StyleUnit,
    pub height: StyleUnit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleRect {
    pub top: StyleUnit,
    pub right: StyleUnit,
    pub bottom: StyleUnit,
    pub left: StyleUnit,
}

#[macro_export]
macro_rules! define_style_props {
    (
        $(
            $(#[$attr:meta])*
            $vis:vis $name:ident : $ty:ty $(= $expr:expr)?
        ),* $(,)?
    ) => {$(
        $(#[$attr])*
        $vis struct $name(pub $ty);
        const _: () = {
            $(impl ::core::default::Default for $name {
                fn default() -> Self {
                    Self($expr)
                }
            })?

            impl ::core::clone::Clone for $name {
                fn clone(&self) -> Self {
                    Self(::core::clone::Clone::clone(&self.0))
                }
            }

            impl ::core::convert::From<$ty> for $name {
                fn from(v: $ty) -> Self {
                    Self(v)
                }
            }

            impl $crate::style::StyleProp for $name {
                const LEVEL: $crate::style::PropLevel = $crate::style::PropLevel::Layout;
            }
        };
    )*};
}

pub use define_style_props;
