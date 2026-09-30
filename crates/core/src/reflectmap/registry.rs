use core::any::TypeId;
use bevy::reflect::Type;
use bevy::{platform::collections::HashMap, prelude::*};


use crate::reflectmap::runtime::*;
use crate::reflectmap::persistence::*;

#[derive(Resource, Default)]
pub struct ReflectMapMarkerRegistry {
    /// Keyed by the declared const identifier on the marker
    pub(crate) by_name: HashMap<&'static str, MarkerInfo>,
    
    /// Keyed by the marker's typepath
    pub(crate) by_typepath: HashMap<&'static str, MarkerInfo>,
}

impl ReflectMapMarkerRegistry {
    pub fn register<M: ReflectMapTypeMarker>(&mut self) {
        let info = MarkerInfo {
            typepath: Type::of::<M>().path(),
            name: M::NAME,
            value_type_id: TypeId::of::<M::Value>(),
            value_type_path: <M::Value as TypePath>::type_path(),
        };
        self.by_name.insert(info.name, info);
        self.by_typepath.insert(info.typepath, info);
    }
}

/// App extension, mirroring register_context_fetcher's ergonomics.
pub trait RegisterReflectMapMarker {
    fn register_map_marker<M: ReflectMapTypeMarker>(&mut self) -> &mut Self;
}

impl RegisterReflectMapMarker for App {
    fn register_map_marker<M: ReflectMapTypeMarker>(&mut self) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<ReflectMapMarkerRegistry>()
            .register::<M>()
        ;
        self
    }
}
