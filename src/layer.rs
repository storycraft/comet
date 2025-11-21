use slotmap::{SecondaryMap, SlotMap, new_key_type};

use crate::layout::LayoutBoxKey;

new_key_type! {
    pub struct LayerKey;
    pub struct TransformKey;
}

type PropertyMap<K, V> = SlotMap<K, Property<V>>;

pub struct LayerTree {
    pub tree: SecondaryMap<LayoutBoxKey, Option<LayerKey>>,
    pub layers: SlotMap<LayerKey, Layer>,
    pub transforms: PropertyMap<TransformKey, ()>,
}

impl LayerTree {
    pub fn new() -> Self {
        Self {
            tree: SecondaryMap::new(),
            layers: SlotMap::with_key(),
            transforms: SlotMap::with_key(),
        }
    }
}

pub struct Layer {
    pub transform: Option<TransformKey>,
}

pub struct Property<T> {
    pub data: T,
}
