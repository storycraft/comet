use hecs::DynamicBundle;

pub trait StyleProp: Sized + Send + Sync + 'static {}

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
