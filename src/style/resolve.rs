use rustc_hash::FxBuildHasher;
use slotmap::{SecondaryMap, SlotMap, SparseSecondaryMap};

use crate::{node::NodeKey, style::{LayoutStyle, LayoutStyleKey}};

pub struct LayoutStore<T: LayoutStyle> {
    pub resolved: SecondaryMap<NodeKey, LayoutStyleKey>,
    pub styles: SlotMap<LayoutStyleKey, T::Resolved>,
    pub map: SparseSecondaryMap<NodeKey, T, FxBuildHasher>,
}

impl<T: LayoutStyle> LayoutStore<T> {
    pub fn new() -> Self {
        Self {
            resolved: SecondaryMap::new(),
            styles: SlotMap::with_key(),
            map: SparseSecondaryMap::with_hasher(FxBuildHasher),
        }
    }
}