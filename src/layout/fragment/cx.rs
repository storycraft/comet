use slotmap::SecondaryMap;
use taffy::Cache;

use crate::layout::input::InputNodeKey;

pub struct LayoutContext {
    caches: SecondaryMap<InputNodeKey, Cache>,
}

impl LayoutContext {
    pub fn new() -> Self {
        Self {
            caches: SecondaryMap::new(),
        }
    }
}
