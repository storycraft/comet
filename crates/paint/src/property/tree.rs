use comet_layout::{fragment::{FragmentKey, tree::FragmentTree}, input::tree::LayoutInputTree};
use comet_tree::ui::Ui;
use slotmap::{SecondaryMap, SlotMap};

use crate::property::{FragmentData, PaintProperties, PropertyKey};

pub struct PaintPropertyTree {
    mapping: SecondaryMap<FragmentKey, FragmentData>,
    properties: SlotMap<PropertyKey, PaintProperties>,
}

impl PaintPropertyTree {
    pub fn new() -> Self {
        Self {
            mapping: SecondaryMap::new(),
            properties: SlotMap::with_key(),
        }
    }

    pub fn resolve(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        fragments: &FragmentTree,
        key: FragmentKey,
        forced: bool,
    ) -> PropertyKey {
        todo!()
    }

    pub fn clear(&mut self) {
        self.mapping.clear();
        self.properties.clear();
    }
}
