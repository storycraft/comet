use core::any::{Any, TypeId};

use nohash_hasher::{IntMap, IsEnabled};
use rustc_hash::FxBuildHasher;
use slotmap::SparseSecondaryMap;

use crate::{layout::GenericLayout, node::NodeKey};

type LayoutStore<T> = SparseSecondaryMap<NodeKey, T, FxBuildHasher>;

pub struct LayoutContainer {
    typemap: IntMap<TypeKey, Box<dyn Any>>,
}

impl LayoutContainer {
    pub fn new() -> Self {
        Self {
            typemap: IntMap::default(),
        }
    }

    fn store<T: GenericLayout>(&self) -> Option<&LayoutStore<T>> {
        self.typemap.get(&TypeKey::of::<T>())?.downcast_ref()
    }

    fn store_mut<T: GenericLayout>(&mut self) -> Option<&mut LayoutStore<T>> {
        self.typemap.get_mut(&TypeKey::of::<T>())?.downcast_mut()
    }

    fn get_or_insert_store_mut<T: GenericLayout>(&mut self) -> &mut LayoutStore<T> {
        self.typemap
            .entry(TypeKey::of::<T>())
            .or_insert_with(|| Box::new(LayoutStore::<T>::default()))
            .downcast_mut()
            .unwrap()
    }

    #[inline]
    pub fn get<T: GenericLayout>(&self, key: NodeKey) -> Option<&T> {
        self.store::<T>()?.get(key)
    }

    #[inline]
    pub fn get_mut<T: GenericLayout>(&mut self, key: NodeKey) -> Option<&mut T> {
        self.store_mut::<T>()?.get_mut(key)
    }

    #[inline]
    pub fn insert<T: GenericLayout>(&mut self, key: NodeKey, style: T) {
        self.get_or_insert_store_mut().insert(key, style);
    }

    #[inline]
    pub fn remove<T: GenericLayout>(&mut self, key: NodeKey) -> Option<T> {
        self.store_mut()?.remove(key)
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
#[repr(transparent)]
pub struct TypeKey(TypeId);

impl TypeKey {
    #[inline]
    pub fn of<T: 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

impl IsEnabled for TypeKey {}
