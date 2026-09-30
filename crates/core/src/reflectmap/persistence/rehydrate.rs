use bevy::reflect::TypeRegistry;
use bevy::reflect::serde::ReflectDeserializer;
use bevy::{prelude::*};
use serde::Deserializer;
use serde::de::DeserializeSeed;

use crate::types::CraniumCow;
use crate::reflectmap::registry::ReflectMapMarkerRegistry;
use crate::reflectmap::runtime::*;
use crate::reflectmap::persistence::*;

#[derive(Debug, Default)]
pub struct RehydrateReport {
    /// Type not present in the registry — a CONTENT gap, not corruption.
    pub unknown_types: Vec<String>,

    /// Marker name not registered — the app is missing a register_map_marker call.
    pub unknown_markers: Vec<String>,

    /// Resolved, but the concrete type disagrees with the marker's Value type.
    pub shape_mismatches: Vec<(String, String, String)>,

    /// Recoverable at the entry level; the rest of the map still loads.
    pub invalid: Vec<(String, String)>,
}

pub fn rehydrate_map<'a, SV: ReflectMapStorageValue + Deserializer<'a>>(
    save: &ReflectMapSave<SV>,
    markers: &ReflectMapMarkerRegistry,
    registry: &TypeRegistry,
    report: &mut RehydrateReport,
) -> ReflectMap 
{
    let mut map = ReflectMap::default();

    for entry in &save.entries {
        // 1. Marker. Missing = the app forgot to register it; skip this entry.
        let Some(info) = markers.by_name.get(entry.marker.as_str()) else {
            report.unknown_markers.push(entry.marker.clone());
            continue;
        };

        // 2. Type. Missing = content the pack doesn't have. Flag, continue - never panic.
        let Some(reg) = registry
            .get_with_type_path(&entry.type_path)
            .or_else(|| registry.get_with_short_type_path(&entry.type_path))
        else {
            report.unknown_types.push(entry.type_path.clone());
            continue;
        };

        // 3. Parse into a DYNAMIC value. The seed needs the registry.
        let deserializer = ReflectDeserializer::new(registry);
        let dynamic: Box<dyn PartialReflect> = match deserializer.deserialize(entry.value.clone()) {
            Ok(v) => v,
            Err(e) => { report.invalid.push((entry.key.clone(), e.to_string())); continue; }
        };

        // 4. Dynamic -> concrete. 
        //    MANDATORY: the result of deserialization is a DynamicStruct, and try_as_reflect() fails on it.
        let Some(from_reflect) = reg.data::<ReflectFromReflect>() else {
            report.invalid.push((entry.key.clone(), "type does not register ReflectFromReflect".into()));
            continue;
        };
        let concrete: Box<dyn Reflect> = match from_reflect.from_reflect(dynamic.as_partial_reflect()) {
            Some(v) => v,
            None => { report.invalid.push((entry.key.clone(), "from_reflect returned None".into())); continue; }
        };

        // 5. Shape check against the marker's declaration. This is where a
        //    pack/save version skew shows up as a diagnosis, not a silent None.
        if concrete.as_any().type_id() != info.value_type_id {
            report.shape_mismatches.push((
                entry.key.clone(),
                info.value_type_path.to_string(),
                concrete.reflect_type_path().to_string(),
            ));
            continue;
        }

        // 6. Insert under the NOW-OWNED key. `Cow::Owned` is the whole reason
        //    the map's key type must be `Cow` and not `&'static str`.
        map.insert_dyn(info.typepath, CraniumCow::Owned(entry.key.clone()), concrete);
    }

    map
}