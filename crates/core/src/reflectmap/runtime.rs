use core::{any::TypeId};

use bevy::{platform::collections::HashMap, prelude::*, reflect::{GetTypeRegistration, Type}};

use crate::types::{CraniumCow, ReflectMapKey};

// #[reflect_trait]
pub trait MarksType {
    type Value: Reflect + Clone + Send + Sync;

    fn get_value_marker() -> TypeId {
        TypeId::of::<Self::Value>()
    }
}

pub trait ReflectMapTypeMarker: 'static + TypePath {
    type Value: Reflect + FromReflect + Clone + Send + Sync + GetTypeRegistration + TypePath + 'static;

    /// An identifier for this marker that survives code migrations (unlike the typepath)
    const NAME: &'static str;
}


#[derive(Default)]
/// A HashMap storing heterogenous Values, where the Key is annotated with the expected Value type for the slot.
/// 
pub struct ReflectMap {
    pub(crate) entries: HashMap<
        (&'static str, ReflectMapKey),  // Typepath of the *Key*
        Box<dyn Reflect>,
    >,
}

impl ReflectMap {
    pub fn get_raw<M: ReflectMapTypeMarker>(&self, key: ReflectMapKey) -> Option<&dyn Reflect> {
        let key_id = Type::of::<M>().path();
        let key_tup = (key_id, key);
        let raw_result = self.entries.get(&key_tup);
        let derefd = raw_result.map(|b| b.as_ref());
        derefd
    }

    pub fn contains<M: ReflectMapTypeMarker>(&self, key: ReflectMapKey) -> bool {
        self.get_raw::<M>(key).is_some()
    }

    pub fn get<M: ReflectMapTypeMarker>(&self, key: ReflectMapKey) -> Option<&M::Value> {
        let raw_result = self.get_raw::<M>(key);
        let result = raw_result.map(|r| r.downcast_ref::<M::Value>()).flatten();
        result
    }

    pub fn insert<M: ReflectMapTypeMarker>(&mut self, key: ReflectMapKey, val: M::Value) {
        let key_id = Type::of::<M>().path();
        let key_tup = (key_id, key);

        let existing = self.entries.insert(
            key_tup, 
            Box::new(val)
        );

        match existing {
            Some(_preex) => {
                #[cfg(feature = "logging")]
                bevy::log::warn!("reflectmap: Overwrote preexisting value {_preex:?}!")
            }
            None => {}
        }
    }

    /// Untyped insert, for the persistence path where the marker is resolved
    /// from data rather than from a type parameter.
    pub fn insert_dyn(
        &mut self, 
        marker: &'static str, 
        key: CraniumCow<'static, str>, 
        value: Box<dyn Reflect>
    ) {
        self.entries.insert((marker, key), value);
    }

    pub fn len(&self) -> usize { self.entries.len() }

    pub fn iter_raw(&self) -> impl Iterator<Item = (&(&'static str, CraniumCow<'static, str>), &dyn Reflect)> {
        self.entries.iter().map(|(k, v)| (k, v.as_reflect()))
    }
}