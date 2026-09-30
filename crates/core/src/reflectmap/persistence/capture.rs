use core::any::TypeId;

use bevy::reflect::TypeRegistry;
use bevy::reflect::serde::ReflectSerializer;
use bevy::{prelude::*};

use crate::reflectmap::registry::ReflectMapMarkerRegistry;
use crate::reflectmap::runtime::*;
use crate::reflectmap::persistence::*;


#[derive(Debug)]
pub enum CaptureError {
    MissingMarker { marker: TypeId, key: String },
    MarkerNotRegistered { name: &'static str },
    Serialize { key: String, source: String },
    /// A value whose type disagrees with its marker's declared Value type.
    ValueTypeMismatch { key: String, expected: &'static str, found: String },
}

pub fn capture_map<SV: ReflectMapStorageValue>(
    map: &ReflectMap,
    markers: &ReflectMapMarkerRegistry,
    registry: &TypeRegistry,
) -> Result<ReflectMapSave<SV>, CaptureError> {
    let mut entries = Vec::with_capacity(map.entries.len());

    for ((typepath, key), value) in map.iter_raw() {
        let info = markers.by_typepath.get(typepath)
            .ok_or(CaptureError::MarkerNotRegistered { name: "<unknown>" })?;

        // Integrity: the stored value must match the marker's declared shape.
        let found = value.reflect_type_path().to_string();
        if value.as_any().type_id() != info.value_type_id {
            return Err(CaptureError::ValueTypeMismatch {
                key: key.to_string(), expected: info.value_type_path, found,
            });
        }

        let ser = ReflectSerializer::new(value.as_partial_reflect(), registry);
        let value_json = SV::try_from_value(&ser)
            .map_err(
                |e| {
                    #[cfg(feature = "logging")]
                    bevy::log::error!("{:?}", e);
                    CaptureError::Serialize { 
                        key: key.to_string(), 
                        source: format!("{:?}", e)
                    }
                }
        )?;

        entries.push(ReflectMapEntrySave {
            marker: info.name.to_string(),
            key: key.to_string(),
            type_path: found,
            value: value_json,
        });
    }

    // I8 — sorted projection [1][3]. The marker is part of the sort key because
    // the same name under two markers is a legitimate, distinct entry.
    entries.sort_by(|a, b| {
        a.marker.cmp(&b.marker).then_with(|| a.key.cmp(&b.key)).then_with(|| a.type_path.cmp(&b.type_path))
    });

    Ok(ReflectMapSave { entries })
}