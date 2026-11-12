use comet_layout::{fragment::{FragmentKey, tree::FragmentTree}, input::tree::LayoutInputTree};
use comet_tree::ui::Ui;
use slotmap::{SecondaryMap, SlotMap};

use crate::property::{FragmentData, PaintProperty, PropertyKey};

pub struct PaintPropertyTree {
    mapping: SecondaryMap<FragmentKey, FragmentData>,
    property: SlotMap<PropertyKey, PaintProperty>,
}

impl PaintPropertyTree {
    pub fn new() -> Self {
        Self {
            mapping: SecondaryMap::new(),
            property: SlotMap::with_key(),
        }
    }

    pub fn resolve(
        &mut self,
        ui: &Ui,
        inputs: &LayoutInputTree,
        fragments: &FragmentTree,
        key: FragmentKey,
        forced: bool,
    ) -> FragmentData {
        todo!()
    }

    pub fn clear(&mut self) {
        self.mapping.clear();
        self.property.clear();
    }
}
