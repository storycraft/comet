use hecs::DynamicBundle;

pub trait StyleProp: Sized + Send + Sync + 'static {
    // TODO: /// Invalidation hint when value is changed
    // fn invalidate(old: Option<&Self>, new: Option<&Self>) -> InvalidateHint;
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum InvalidateHint {
    /// No work is required
    None,
    /// Redraw is required
    Paint,
    /// Node's layout is invalidated
    Layout,
    /// Need to rebuild layout tree from the nearest block ancestor
    FullLayout,
}

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
