//! The type registry, used by ReflectMaps to handle restoring reflected types back to concrete ones.

use core::any::TypeId;
use bevy::reflect::Type;
use bevy::{platform::collections::HashMap, prelude::*};

use crate::reflectmap::runtime::*;
use crate::reflectmap::persistence::*;

/// An App-level registry of [`ReflectMapTypeMarker`] types, which 
/// allows 
#[derive(Resource, Default)]
pub struct ReflectMapMarkerRegistry {
    /// Keyed by the declared const identifier on the marker
    pub(crate) by_name: HashMap<&'static str, MarkerInfo>,
    
    /// Keyed by the marker's typepath
    pub(crate) by_typepath: HashMap<&'static str, MarkerInfo>,
}

impl ReflectMapMarkerRegistry {
    /// Register the provided marker type with the registry. 
    /// 
    /// This makes two entries - by the NAME const and by typepath.
    /// 
    /// The MarkerInfo struct that serves as the record provides both, 
    /// so you can trivially convert between the two representations.
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

    /// Retrieves [`MarkerInfo`] for the specified marker typepath, if registered.
    pub fn get_by_typepath(&self, typepath: &str) -> Option<&MarkerInfo> {
        self.by_typepath.get(typepath)
    }

    /// Retrieves [`MarkerInfo`] for the specified [`ReflectMapTypeMarker::NAME`] name, if registered.
    pub fn get_by_name(&self, typepath: &str) -> Option<&MarkerInfo> {
        self.by_typepath.get(typepath)
    }

    /// Retrieves the name for the specified [`ReflectMapTypeMarker`] typepath, if registered.
    pub fn typepath_to_name(&self, typepath: &str) -> Option<&str> {
        self.get_by_typepath(typepath).map(|i| i.name)
    }

    /// Retrieves the typepath for the specified [`ReflectMapTypeMarker::NAME`], if registered.
    pub fn name_to_typepath(&self, name: &str) -> Option<&str> {
        self.get_by_name(name).map(|i| i.typepath)
    }

    /// Returns True if the provided [`ReflectMapTypeMarker`] type `M` is registered 
    /// and can mark values of the provided type `T` in [`ReflectMap`]s.
    /// 
    /// See also [`ReflectMapTypeMarker::marks_type`] and [`ReflectMapTypeMarker::marks_type_of_value`] 
    /// for cases where you don't care whether the marker is registered properly or not.
    pub fn marks_type<M: ReflectMapTypeMarker, T: TypePath>(&self) -> bool {
        let target = T::type_path();
        let key = M::type_path();
        
        self.by_typepath
            .get(&key)
            .map(|v| v.value_type_path == target)
            .unwrap_or(false)
    }
}

/// App extension, mirroring register_context_fetcher's ergonomics.
pub trait RegisterReflectMapMarker {
    fn register_map_marker<M: ReflectMapTypeMarker>(&mut self) -> &mut Self;
}

impl RegisterReflectMapMarker for App {
    /// Registers the provided type as a [`ReflectMapTypeMarker`], allowing it to be used for 
    /// ReflectMaps that may be saved and reloaded via a SerDe of some kind safely.
    /// 
    /// **NOTE:** While this is currently not enforced, it is *STRONGLY* recommended to register 
    ///           ALL types that you use as markers, even if you do not currently SerDe the map,
    ///           as there is no guarantee that this will not be enforced in future releases.
    fn register_map_marker<M: ReflectMapTypeMarker>(&mut self) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<ReflectMapMarkerRegistry>()
            .register::<M>()
        ;
        self
    }
}
