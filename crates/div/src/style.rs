use hecs::DynamicBundle;

pub trait StyleProp: Sized + Send + Sync + 'static {
    const LEVEL: PropLevel;
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum PropLevel {
    /// Redraw is required
    Paint,
    /// Node's layout is invalidated
    Layout,
    /// Need to rebuild layout tree from the nearest block ancestor
    FullLayout,
}

#[macro_export]
macro_rules! style_prop {
    ($ty:ty = $level:expr) => {
        impl $crate::style::StyleProp for $ty {
            const LEVEL: $crate::style::PropLevel = $level;
        }
    };
}
pub use style_prop;

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
                const LEVEL: $crate::style::PropLevel = $crate::style::PropLevel::FullLayout;
            }
        };
    )*};
}
pub use define_style_props;

pub trait StyleProps: DynamicBundle {}

macro_rules! impl_tuples {
    (@impl $($t:ident)*) => {
        impl<$($t: $crate::style::StyleProp,)*> $crate::style::StyleProps for ($($t,)*) {}
    };

    (@accum [$($t:ident)*]) => {
        impl_tuples!(@impl $($t)*);
    };

    (@accum [$($t:ident)*] $next:ident $($rest:tt)*) => {
        impl_tuples!(@impl $($t)*);
        impl_tuples!(@accum [$($t)* $next] $($rest)*);
    };

    ($($t:ident)*) => {
        impl_tuples!(@accum [] $($t)*);
    };
}

impl_tuples!(A B C D E F G H I J K L M N O);
