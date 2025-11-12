use hecs::{Component, ComponentRef, DynamicBundle, Entity, EntityRef, World};
use slotmap::SecondaryMap;

use crate::node::NodeKey;

pub struct PropStore {
    world: World,
    map: SecondaryMap<NodeKey, Entity>,
}

impl PropStore {
    pub fn new() -> Self {
        Self {
            world: World::new(),
            map: SecondaryMap::new(),
        }
    }

    pub fn add_props(&mut self, key: NodeKey, props: impl DynamicBundle) {
        match self.map.get(key) {
            Some(&entity) => {
                _ = self.world.insert(entity, props);
            }
            None => {
                let entity = self.world.reserve_entity();
                self.map.insert(key, entity);
                self.world.spawn_at(entity, props);
            }
        };
    }

    pub fn props(&self, key: NodeKey) -> Option<Props> {
        let entity = *self.map.get(key)?;
        Some(Props(self.world.entity(entity).ok()?))
    }

    pub fn remove<T: Component>(&mut self, key: NodeKey) -> Option<T> {
        let entity = *self.map.get(key)?;
        self.world.remove_one::<T>(entity).ok()
    }

    pub fn remove_all(&mut self, key: NodeKey) {
        let Some(entity) = self.map.remove(key) else {
            return;
        };
        _ = self.world.despawn(entity);
    }

    pub fn get<'a, T: ComponentRef<'a>>(&'a self, key: NodeKey) -> Option<T::Ref> {
        let entity = *self.map.get(key)?;
        self.world.get::<T>(entity).ok()
    }

    pub fn get_cloned<T: Component + Clone>(&self, key: NodeKey) -> Option<T> {
        Some((*self.get::<&T>(key)?).clone())
    }
}

pub struct Props<'a>(EntityRef<'a>);

impl<'a> Props<'a> {
    #[inline]
    pub fn get<T: ComponentRef<'a>>(&self) -> Option<T::Ref> {
        self.0.get::<T>()
    }

    pub fn get_cloned<T: Component + Clone>(&self) -> Option<T> {
        Some((*self.get::<&T>()?).clone())
    }
}
