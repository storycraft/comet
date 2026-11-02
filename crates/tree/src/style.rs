use hecs::DynamicBundle;

pub trait StyleProp: Sized + Send + Sync + 'static {}

#[macro_export]
macro_rules! define_style_props {
    (
        $(
            $(#[$attr:meta])*
            $vis:vis $name:ident : $ty:ty $(= $expr:expr)?
        ),* $(,)?
    ) => {$(
        $(#[$attr])*
        #[repr(transparent)]
        $vis struct $name(pub $ty);

        $(impl ::core::default::Default for $name {
            fn default() -> Self {
                Self($expr)
            }
        })?

        impl ::core::convert::From<$ty> for $name {
            fn from(v: $ty) -> Self {
                Self(v)
            }
        }

        impl $crate::style::StyleProp for $ty {}
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
