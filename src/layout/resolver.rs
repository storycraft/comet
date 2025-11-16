use parley::TextStyle;
use rustc_hash::FxHashMap;
use slotmap::{SlotMap, new_key_type};

use crate::ui::NodeKey;

new_key_type! {
    pub struct ResolvedTextStyleKey;
}

/// Compactly store resolved text styles
pub struct TextStyleResolver {
    pub styles: SlotMap<ResolvedTextStyleKey, TextStyle<'static, NodeKey>>,
    pub map: FxHashMap<NodeKey, ResolvedTextStyleKey>,
}

impl TextStyleResolver {
    pub fn new() -> Self {
        Self {
            styles: SlotMap::with_key(),
            map: FxHashMap::default(),
        }
    }

    /// Invalidate resolved text styles for key
    pub fn invalidate(&mut self, key: NodeKey) {
        let Some(style_key) = self.map.remove(&key) else {
            return;
        };

        if let Some(resolved_style) = self.styles.get(style_key)
            && resolved_style.brush == key
        {
            self.styles.remove(style_key);
        }
    }
}
