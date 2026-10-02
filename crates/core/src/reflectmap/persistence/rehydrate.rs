use bevy::reflect::TypeRegistry;
use bevy::reflect::serde::ReflectDeserializer;
use bevy::{prelude::*};
use serde::{Serialize, Deserialize};
use serde::Deserializer;
use serde::de::DeserializeSeed;

use crate::types::CraniumCow;
use crate::reflectmap::registry::ReflectMapMarkerRegistry;
use crate::reflectmap::runtime::*;
use crate::reflectmap::persistence::*;

/// A container for capturing Stuff Going Wrong with [`ReflectMap`] rehydration as done by 
/// the [`rehydrate_map`] function and its wrapper siblings.
/// 
/// This is effectively little more than a 'lazy log' that is a little bit more machine-readable.
/// 
/// The whole thing is also SerDe-able both ways in case you want to save it somewhere.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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

/// Restores a SerDe-ified [`ReflectMapSave`] value to a proper, runtime [`ReflectMap`].
/// 
/// The [`ReflectMapSave`] is only borrowed, not consumed - the restored map is effectively 
/// cloned from the provided SerDe snapshot.
/// 
/// See also [`super::capture::capture_map`], which is the dual of this function.
/// 
/// If a [`RehydrateReport`] is provided, it will be updated with entries about any rehydration 
/// failures (in-place). If not, it will be passed through.
/// 
/// See also [`rehydrate_map_with_new_report`] which is a convenience method that creates and 
/// returns the report for you alongside the ReflectMap, or [`rehydrate_map_without_report`] 
/// if you don't want one at all.
pub fn rehydrate_map<'a, SV: ReflectMapStorageValue + Deserializer<'a>>(
    save: &ReflectMapSave<SV>,
    markers: &ReflectMapMarkerRegistry,
    registry: &TypeRegistry,
    mut report: Option<&mut RehydrateReport>,
) -> ReflectMap 
{
    let mut map = ReflectMap::default();

    for entry in &save.entries {
        // 1. Marker. Missing = the app forgot to register it; skip this entry.
        let Some(info) = markers.by_name.get(entry.marker.as_str()) else {
            report.as_mut().map(|m| m.unknown_markers.push(entry.marker.clone()));
            continue;
        };

        // 2. Type. Missing = content the pack doesn't have. Flag, continue - never panic.
        let Some(reg) = registry
            .get_with_type_path(&entry.type_path)
            .or_else(|| registry.get_with_short_type_path(&entry.type_path))
        else {
            report.as_mut().map(|m| m.unknown_types.push(entry.type_path.clone()));
            continue;
        };

        // 3. Parse into a DYNAMIC value. The seed needs the registry.
        let deserializer = ReflectDeserializer::new(registry);
        let dynamic: Box<dyn PartialReflect> = match deserializer.deserialize(entry.value.clone()) {
            Ok(v) => v,
            Err(e) => { 
                report.as_mut().map(|m| m.invalid.push((entry.key.clone(), e.to_string()))); 
                continue; 
            }
        };

        // 4. Dynamic -> concrete. 
        //    MANDATORY: the result of deserialization is a DynamicStruct, 
        //               and try_as_reflect() fails on it.
        let Some(from_reflect) = reg.data::<ReflectFromReflect>() else {
            report.as_mut().map(|m| m.invalid.push((entry.key.clone(), "type does not register ReflectFromReflect".into())));
            continue;
        };

        let concrete: Box<dyn Reflect> = match from_reflect.from_reflect(dynamic.as_partial_reflect()) {
            Some(v) => v,
            None => { 
                report.as_mut().map(|m| m.invalid.push((entry.key.clone(), "from_reflect returned None".into()))); 
                continue; 
            }
        };

        // 5. Shape check against the marker's declaration. This is where a
        //    pack/save version skew shows up as a diagnosis, not a silent None.
        if concrete.as_any().type_id() != info.value_type_id {
            report.as_mut().map(|m| m.shape_mismatches.push((
                entry.key.clone(),
                info.value_type_path.to_string(),
                concrete.reflect_type_path().to_string(),
            )));
            continue;
        }

        // 6. Insert under the NOW-OWNED key. `Cow::Owned` is the whole reason
        //    the map's key type must be `Cow` and not `&'static str`.
        let marker_key = cfg_select! {
            feature = "reflectmap_keys_by_name_const" => { info.name },
            _ => info.typepath
        };

        map.insert_dyn(marker_key, CraniumCow::Owned(entry.key.clone()), concrete);
    }

    map
}

/// Restores a SerDe-ified [`ReflectMapSave`] value to a proper, runtime [`ReflectMap`].
/// 
/// See [`rehydrate_map`] for details - this is a helper that simplifies the API for handling 
/// the [`RehydrateReport`] audit reports - it will create a new one for you and return it 
/// with the ReflectMap result.
/// 
/// See also  [`rehydrate_map_without_report`] if you don't want an audit report at all.
pub fn rehydrate_map_with_new_report<'a, SV: ReflectMapStorageValue + Deserializer<'a>>(
    save: &ReflectMapSave<SV>,
    markers: &ReflectMapMarkerRegistry,
    registry: &TypeRegistry,
) -> (ReflectMap, RehydrateReport) {
    let mut report = RehydrateReport::default();
    let result = rehydrate_map(save, markers, registry, Some(&mut report));
    (result, report)
}

/// Restores a SerDe-ified [`ReflectMapSave`] value to a proper, runtime [`ReflectMap`].
/// 
/// See [`rehydrate_map`] for details - this is a helper that simplifies the API for handling 
/// the [`RehydrateReport`] audit reports - it will simply pass in a None value, which ignores 
/// any steps that would populate the report.
/// 
/// See also  [`rehydrate_map_with_new_report`] if you do want an audit report.
pub fn rehydrate_map_without_report<'a, SV: ReflectMapStorageValue + Deserializer<'a>>(
    save: &ReflectMapSave<SV>,
    markers: &ReflectMapMarkerRegistry,
    registry: &TypeRegistry,
) -> ReflectMap {
    let result = rehydrate_map(save, markers, registry, None);
    result
}
