use slotmap::SecondaryMap;
use taffy::Cache;

use crate::layout::input::InputNodeKey;

pub struct LayoutContext {
    caches: SecondaryMap<InputNodeKey, Cache>,
}

impl Default for LayoutContext {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutContext {
    pub fn new() -> Self {
        Self {
            caches: SecondaryMap::new(),
        }
    }
}
