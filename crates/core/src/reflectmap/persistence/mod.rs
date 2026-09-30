pub mod capture;
pub mod rehydrate;

use core::{any::TypeId, fmt::Debug};

use bevy::{prelude::*};

use serde::{Serialize, Deserialize};
use serde_json;

/// The broad-church serializable generic Value type, like serde_json::Value.
/// This is a trait to allow other types to possibly register here.
pub trait ReflectMapStorageValue: Serialize + Clone {
    type Error: Debug;

    fn try_from_value<T: Serialize>(val: T) -> Result<Self, Self::Error> where Self: Sized;
}

impl ReflectMapStorageValue for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from_value<T: Serialize>(val: T) -> Result<Self, Self::Error> {
        // Note that our naming convention is flipped from serde_json's.
        // They use 'to_value()' because they talk about serde_json::Values, 
        // for us the INPUT is the value and the output type is hidden from the trait.
        serde_json::to_value(val)
    }
}


/// One persisted slot. Three-part identity: marker, name, concrete value type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectMapEntrySave<SV: ReflectMapStorageValue> {
    /// Registered marker name (e.g. "ActorSlot"); the const NAME field on the marker. 
    /// Can be used as a key for `ReflectMapMarkerRegistry::by_name()` to retrieve metadata.
    pub marker: String,

    /// The slot name, i.e. the actual Key in the map without the type-marker (e.g. "Enemy").
    pub key: String,

    /// Fully-qualified TypePath of the concrete value. Stored EXPLICITLY so the
    /// load can validate before attempting to deserialize the body, and so an
    /// unknown type is a diagnosable skip rather than a parse error.
    pub type_path: String,

    /// The format's dynamic Value type. This is the only format-specific field.
    pub value: SV,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReflectMapSave<SV: ReflectMapStorageValue> {
    /// Sorted by `(marker, key, type_path)` on capture — I8 [1][3].
    pub entries: Vec<ReflectMapEntrySave<SV>>,
}

#[derive(Clone, Copy)]
pub struct MarkerInfo {
    pub typepath: &'static str,
    pub name: &'static str,
    pub value_type_id: TypeId,
    pub value_type_path: &'static str,
}

#[cfg(test)]
mod test {
    use bevy::reflect::{TypeRegistry, serde::ReflectSerializer};
    use super::*;
    use crate::reflectmap::ReflectMap;
    use crate::reflectmap::persistence::capture::{capture_map};
    use crate::reflectmap::persistence::rehydrate::{RehydrateReport, rehydrate_map};
    use crate::reflectmap::registry::{ReflectMapMarkerRegistry};
    use crate::reflectmap::test::*;

    fn registry() -> TypeRegistry {
        let mut r = TypeRegistry::default();
        r.register::<AgentHandle>();
        r.register::<Vec<AgentHandle>>();
        r.register::<Position>();
        r.register::<Goal>();
        r
    }

    fn markers() -> ReflectMapMarkerRegistry {
        let mut m = ReflectMapMarkerRegistry::default();
        m.register::<ActorSlot>();
        m.register::<EnemyOneSlot>();
        m.register::<EnemyManySlot>();
        m.register::<PositionSlot>();
        m.register::<GoalSlot>();
        m
    }

    fn assert_clean(report: &RehydrateReport) {
        assert!(report.unknown_markers.is_empty(), "unknown_markers: {report:?}");
        assert!(report.unknown_types.is_empty(), "unknown_types: {report:?}");
        assert!(report.shape_mismatches.is_empty(), "shape_mismatches: {report:?}");
        assert!(report.invalid.is_empty(), "invalid: {report:?}");
    }

    /// capture → json → parse → rehydrate, asserting the report is clean.
    fn round_trip(
        map: &ReflectMap,
        markers: &ReflectMapMarkerRegistry,
        registry: &TypeRegistry,
    ) -> ReflectMap {
        let save = capture_map::<serde_json::Value>(map, markers, registry).expect("capture");
        let json = serde_json::to_string(&save).expect("serialize");
        let loaded: ReflectMapSave<serde_json::Value> = serde_json::from_str(&json).expect("parse");

        let mut report = RehydrateReport::default();
        let restored = rehydrate_map(&loaded, markers, registry, &mut report);
        assert_clean(&report);
        restored
    }

    /// Builds a one-entry save by hand. Needed because the TYPED insert API makes
    /// a shape mismatch unrepresentable — only a corrupt or older blob can carry
    /// one, so the test has to synthesise it at the blob layer.
    fn synthesised_entry(
        marker: &str,
        slot: &str,
        value: &dyn bevy::reflect::PartialReflect,
        registry: &TypeRegistry,
    ) -> ReflectMapSave<serde_json::Value> {
        let ser = ReflectSerializer::new(value, registry);
        ReflectMapSave {
            entries: vec![crate::reflectmap::persistence::ReflectMapEntrySave {
                marker: marker.to_string(),
                key: slot.to_string(),
                type_path: value.reflect_type_path().to_string(),
                value: serde_json::to_value(&ser).expect("serialize body"),
            }],
        }
    }

    #[test]
    fn reflect_map_round_trips_through_json() {
        let (registry, markers) = (registry(), markers());

        let mut map = ReflectMap::default();
        map.insert::<ActorSlot>(key("actor"), AgentHandle(7));
        map.insert::<EnemyManySlot>(key("enemies"), vec![AgentHandle(4), AgentHandle(5)]);
        map.insert::<EnemyOneSlot>(key("enemies"), AgentHandle(3));
        map.insert::<PositionSlot>(key("home"), Position { x: 3, y: -4 });
        map.insert::<GoalSlot>(key("goal"), Goal { label: "sleep".into(), weight: 0.5 });

        let restored = round_trip(&map, &markers, &registry);

        assert_eq!(restored.len(), 5);
        assert_eq!(restored.get::<ActorSlot>("actor".into()), Some(&AgentHandle(7)));
        assert_eq!(restored.get::<PositionSlot>("home".into()), Some(&Position { x: 3, y: -4 }));
        assert_eq!(
            restored.get::<GoalSlot>("goal".into()),
            Some(&Goal { label: "sleep".into(), weight: 0.5 })
        );
    }

    /// The test that proves persisting the MARKER was necessary: a save that
    /// recorded only the slot name would collapse these two into one.
    #[test]
    fn reflect_map_round_trips_all_variants_of_a_union_key() {
        let (registry, markers) = (registry(), markers());

        let mut map = ReflectMap::default();
        map.insert::<EnemyOneSlot>(key("enemies"), AgentHandle(3));
        map.insert::<EnemyManySlot>(key("enemies"), vec![AgentHandle(4), AgentHandle(5)]);

        let restored = round_trip(&map, &markers, &registry);

        assert_eq!(restored.len(), 2, "both variants survive the round trip");
        assert_eq!(restored.get::<EnemyOneSlot>("enemies".into()), Some(&AgentHandle(3)));
        assert_eq!(
            restored.get::<EnemyManySlot>("enemies".into()),
            Some(&vec![AgentHandle(4), AgentHandle(5)])
        );
    }

    /// I8: any persisted map→Vec projection is sorted before write, so two maps
    /// with identical contents produce byte-identical blobs [1].
    #[test]
    fn capture_is_insertion_order_invariant() {
        let (registry, markers) = (registry(), markers());

        let build = |order: &[(&'static str, &'static str)]| -> String {
            let mut map = ReflectMap::default();
            for (slot, name) in order {
                match *slot {
                    "actor" => map.insert::<ActorSlot>(key(name), AgentHandle(1)),
                    "enemy" => map.insert::<EnemyOneSlot>(key(name), AgentHandle(2)),
                    other => panic!("unknown fixture slot {other}"),
                }
            }
            let save = capture_map::<serde_json::Value>(&map, &markers, &registry).expect("capture");
            serde_json::to_string(&save).expect("serialize")
        };

        let a = build(&[("actor", "z"), ("enemy", "a"), ("actor", "a")]);
        let b = build(&[("actor", "a"), ("enemy", "a"), ("actor", "z")]);

        assert_eq!(a, b, "capture must not leak insertion order into the blob (I8)");
    }

    #[test]
    fn an_empty_map_round_trips() {
        let (registry, markers) = (registry(), markers());

        let restored = round_trip(&ReflectMap::default(), &markers, &registry);

        assert!(restored.entries.is_empty());
    }

    /// A CONTENT gap: the app knows the marker, the pack no longer ships the type.
    /// Flag and continue — never fatal (the Cranium-side analogue of Hamartia's
    /// MissingContent contract).
    #[test]
    fn rehydrate_flags_an_unknown_type_and_keeps_the_rest() {
        let (registry, markers) = (registry(), markers());

        let mut map = ReflectMap::default();
        map.insert::<ActorSlot>(key("actor"), AgentHandle(7));
        map.insert::<PositionSlot>(key("home"), Position { x: 1, y: 2 });

        let save = capture_map::<serde_json::Value>(&map, &markers, &registry).expect("capture");

        // A registry that knows the markers but not Position.
        let mut lean = TypeRegistry::default();
        lean.register::<AgentHandle>();
        lean.register::<Vec<AgentHandle>>();

        let mut report = RehydrateReport::default();
        let restored = rehydrate_map(&save, &markers, &lean, &mut report);

        assert_eq!(
            restored.get::<ActorSlot>("actor".into()),
            Some(&AgentHandle(7)),
            "a good entry must still load"
        );
        assert_eq!(restored.get::<PositionSlot>("home".into()), None);
        assert_eq!(report.unknown_types.len(), 1, "the miss is diagnosed, not silent");
        assert!(report.unknown_types[0].contains("Position"));
    }

    /// An APP bug, distinct from the content bug above: the app forgot its
    /// `register_map_marker` call. Different cause, different report field.
    #[test]
    fn rehydrate_flags_a_marker_the_app_forgot_to_register() {
        let (registry, markers) = (registry(), markers());

        let mut map = ReflectMap::default();
        map.insert::<ActorSlot>(key("actor"), AgentHandle(7));
        let save = capture_map::<serde_json::Value>(&map, &markers, &registry).expect("capture");

        let mut lean_markers = ReflectMapMarkerRegistry::default();
        lean_markers.register::<EnemyManySlot>();

        let mut report = RehydrateReport::default();
        let restored = rehydrate_map(&save, &lean_markers, &registry, &mut report);

        assert!(restored.entries.is_empty());
        assert_eq!(report.unknown_markers, vec!["ActorSlot".to_string()]);
        assert!(
            report.unknown_types.is_empty(),
            "the type WAS known — this is a registration gap, and the report must say so"
        );
    }

    /// A blob whose slot declares one shape but carries another. Unrepresentable
    /// through the typed API, so it is synthesised at the blob layer.
    #[test]
    fn rehydrate_rejects_a_shape_mismatch() {
        let (registry, markers) = (registry(), markers());

        let save = synthesised_entry(
            "ActorSlot",                       // declares Value = AgentHandle
            "actor",
            &Position { x: 9, y: 9 },          // carries a Position
            &registry,
        );

        let mut report = RehydrateReport::default();
        let restored = rehydrate_map(&save, &markers, &registry, &mut report);

        assert!(restored.entries.is_empty());
        assert_eq!(
            report.shape_mismatches.len(),
            1,
            "a version-skewed blob is a diagnosis, not a silent None: {report:?}"
        );
        assert!(report.invalid.is_empty(), "resolvable types are not 'invalid'");
    }

    /// A blob naming a type no one registered, in a registry that still has the
    /// marker — a content gap reached through the marker path rather than past it.
    #[test]
    fn rehydrate_reports_an_unregistered_value_type_as_a_content_gap() {
        let (registry, markers) = (registry(), markers());

        let save = synthesised_entry("ActorSlot", "actor", &AgentHandle(1), &registry);

        let lean = TypeRegistry::default(); // knows nothing, not even AgentHandle

        let mut report = RehydrateReport::default();
        let restored = rehydrate_map(&save, &markers, &lean, &mut report);

        assert!(restored.entries.is_empty());
        assert_eq!(report.unknown_types.len(), 1);
        assert!(report.shape_mismatches.is_empty(), "unknown is not the same as wrong-shaped");
    }

    #[test]
    fn capture_does_not_flag_entity_free_values() {
        let (registry, markers) = (registry(), markers());

        let mut map = ReflectMap::default();
        map.insert::<ActorSlot>(key("actor"), AgentHandle(7));
        map.insert::<EnemyManySlot>(key("enemies"), vec![AgentHandle(4)]);

        assert!(
            capture_map::<serde_json::Value>(&map, &markers, &registry).is_ok(),
            "the guard must not false-positive on id-bearing values"
        );
    }

    /// The dynamic→concrete step is mandatory and silent when skipped: a
    /// deserialized value is a `DynamicStruct`, and `try_as_reflect` fails on it
    /// [2]. If `rehydrate_map` ever drops the `ReflectFromReflect` call, every
    /// `get::<M>` becomes `None` while the report stays clean — this pins that.
    #[test]
    fn rehydrated_values_are_concrete_not_dynamic() {
        let (registry, markers) = (registry(), markers());

        let mut map = ReflectMap::default();
        map.insert::<PositionSlot>(key("home"), Position { x: 1, y: 2 });

        let restored = round_trip(&map, &markers, &registry);

        let restored_value = restored
            .get_raw::<PositionSlot>("home".into())
            .expect("slot present")
            .as_reflect();

        assert!(
            restored_value.try_as_reflect().is_some(),
            "value is still dynamic — the ReflectFromReflect step was skipped"
        );
        assert!(!restored_value.is_dynamic());
    }
}
