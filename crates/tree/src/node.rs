use core::num::NonZeroU64;

use hecs::Entity;

use crate::style_prop;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Node {
    /// Rectangle box
    Box,
    /// Text
    Text,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct NodeKey(pub(crate) Entity);

impl NodeKey {
    pub const fn bits(self) -> NonZeroU64 {
        self.0.to_bits()
    }

    /// Entity index only; use `bits` when generation-sensitive identity matters.
    pub const fn id(self) -> u32 {
        self.0.id()
    }
}

#[derive(Debug, Clone, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Text(pub String);
style_prop!(Text);
