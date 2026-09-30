
/// 
pub mod registry;

/// The actual datastructure
pub mod runtime;

/// The built-in marker types
pub mod markers;

pub use registry::*;
pub use runtime::*;
pub use markers::*;

#[cfg(feature = "reflectmap_persistence")]
/// SerDe mechanisms for save/load
pub mod persistence;

#[cfg(test)]
mod test {
    //! Tests for `ReflectMap` and its persistence layer.
    //!
    //! We have three groups:
    //!   1. In-memory semantics      — no feature gate, no App.
    //!   2. Persistence round-trips  — gated on the persistence feature.
    //!   3. Registration ergonomics  — needs an `App` only for the extension trait.
    //!
    //! Group (2) lives under `persistence/mod.rs` as it's only relevant when that module is included.
    //! 
    //! Deliberately App-free where it can be. This is a datastructure, not a Bevy system
    //! (in all honesty this may get split off into its own crate someday!).

    use bevy::prelude::*;
    use bevy::reflect::Reflect;

    use crate::reflectmap::{ReflectMap, ReflectMapMarkerRegistry, ReflectMapTypeMarker, RegisterReflectMapMarker};
    use crate::types::CraniumCow;

    // ---------------------------------------------------------------- fixtures

    /// Stands in for an `AgentId`. Deliberately NOT an `Entity` — the analogue of
    /// I3: persisted references are ids, never handles [1].
    #[derive(Debug, Clone, PartialEq, Reflect)]
    pub(crate) struct AgentHandle(pub(crate) u32);

    #[derive(Debug, Clone, PartialEq, Reflect)]
    pub(crate) struct Position {
        pub(crate) x: i32,
        pub(crate) y: i32,
    }

    #[derive(Debug, Clone, PartialEq, Reflect)]
    pub(crate) struct Goal {
        pub(crate) label: String,
        /// Kept exactly representable in binary so the round-trip equality is a
        /// statement about the serializer, not about float rounding.
        pub(crate) weight: f32,
    }

    /// The actor slot.
    #[derive(TypePath)]
    pub(crate) struct ActorSlot;


    impl ReflectMapTypeMarker for ActorSlot {
        type Value = AgentHandle;
        const NAME: &'static str = "ActorSlot";
    }

    /// Union variant A — "enemies" resolved to a single id.
    #[derive(TypePath)]
    pub(crate) struct EnemyOneSlot;

    impl ReflectMapTypeMarker for EnemyOneSlot {
        type Value = AgentHandle;
        const NAME: &'static str = "EnemyOneSlot";
    }

    /// Union variant B — "enemies" resolved to many ids.
    ///
    /// Note the NAME differs from `EnemyOneSlot`'s: NAME identifies the MARKER, not
    /// the slot. The union is expressed by both writing the same SLOT name.
    #[derive(TypePath)]
    pub(crate) struct EnemyManySlot;

    impl ReflectMapTypeMarker for EnemyManySlot {
        type Value = Vec<AgentHandle>;
        const NAME: &'static str = "EnemyManySlot";
    }

    #[derive(TypePath)]
    pub(crate) struct PositionSlot;

    impl ReflectMapTypeMarker for PositionSlot {
        type Value = Position;
        const NAME: &'static str = "PositionSlot";
    }

    #[derive(TypePath)]
    pub(crate) struct GoalSlot;

    impl ReflectMapTypeMarker for GoalSlot {
        type Value = Goal;
        const NAME: &'static str = "GoalSlot";
    }

    /// Adapts a literal to the map's key type.
    ///
    /// The single adaptation seam for the `Cow`-in-std / `&'static str`-in-no_std
    /// split: change the return type and body if you kept the latter.
    pub(crate) fn key(s: &'static str) -> CraniumCow<'static, str> {
        CraniumCow::Borrowed(s)
    }

    // -------------------------------------------------- 1. in-memory semantics

    #[test]
    fn insert_then_get_returns_the_value() {
        let mut map = ReflectMap::default();
        map.insert::<ActorSlot>(key("actor"), AgentHandle(7));

        assert_eq!(map.get::<ActorSlot>("actor".into()), Some(&AgentHandle(7)));
    }

    #[test]
    fn get_on_an_absent_slot_returns_none() {
        let map = ReflectMap::default();

        assert_eq!(map.get::<ActorSlot>("nobody".into()), None);
    }

    /// The discriminating case: the NAME is present, the MARKER is not. This is what
    /// makes a miss diagnosable rather than silent.
    #[test]
    fn a_name_under_a_different_marker_is_a_distinct_slot() {
        let mut map = ReflectMap::default();
        map.insert::<EnemyOneSlot>(key("enemies"), AgentHandle(3));

        assert_eq!(
            map.get::<EnemyManySlot>("enemies".into()),
            None,
            "a different marker over the same name is a different slot"
        );
        assert!(
            map.contains::<EnemyOneSlot>("enemies".into()),
            "but the name IS present — the miss is a shape mismatch, not an absence"
        );
    }

    #[test]
    fn inserting_the_same_marker_and_name_twice_overwrites() {
        let mut map = ReflectMap::default();
        map.insert::<ActorSlot>(key("actor"), AgentHandle(1));
        map.insert::<ActorSlot>(key("actor"), AgentHandle(2));

        // Last-wins is the contract (the impl warns on ejection). Pinned here so a
        // future "ignore the duplicate" refactor is a deliberate change, not a drift.
        assert_eq!(map.get::<ActorSlot>("actor".into()), Some(&AgentHandle(2)));
        assert_eq!(map.len(), 1);
    }

    /// The union capability: one SLOT name, two MARKERS, both stored, both readable.
    #[test]
    fn union_variants_coexist_under_one_name() {
        let mut map = ReflectMap::default();
        map.insert::<EnemyOneSlot>(key("enemies"), AgentHandle(3));
        map.insert::<EnemyManySlot>(key("enemies"), vec![AgentHandle(4), AgentHandle(5)]);

        assert_eq!(map.len(), 2, "the two variants must coexist, not collapse");
        assert_eq!(map.get::<EnemyOneSlot>("enemies".into()), Some(&AgentHandle(3)));
        assert_eq!(
            map.get::<EnemyManySlot>("enemies".into()),
            Some(&vec![AgentHandle(4), AgentHandle(5)])
        );
    }

    /// The realistic union read: the fetcher filled ONE shape, the handler probes
    /// both and takes whichever resolves.
    #[test]
    fn a_handler_probing_variants_resolves_whichever_shape_was_fetched() {
        let mut map = ReflectMap::default();
        map.insert::<EnemyOneSlot>(key("enemies"), AgentHandle(3));

        let as_many = map
            .get::<EnemyManySlot>("enemies".into())
            .cloned()
            .or_else(|| map.get::<EnemyOneSlot>("enemies".into()).map(|h| vec![h.clone()]));

        assert_eq!(as_many, Some(vec![AgentHandle(3)]));
    }

    #[test]
    fn a_fresh_map_is_empty() {
        let map = ReflectMap::default();

        assert!(map.entries.is_empty());
        assert_eq!(map.len(), 0);
    }

    // Group 3: registration ergonomics
    #[test]
    fn register_map_marker_populates_the_registry() {
        let mut app = App::new();
        app.register_map_marker::<ActorSlot>();
        app.register_map_marker::<EnemyManySlot>();

        let registry = app.world().resource::<ReflectMapMarkerRegistry>();
        assert!(registry.by_name.contains_key("ActorSlot"));
        assert!(registry.by_name.contains_key("EnemyManySlot"));
    }

    #[test]
    fn registering_a_marker_twice_is_harmless() {
        let mut app = App::new();
        app.register_map_marker::<ActorSlot>();
        app.register_map_marker::<ActorSlot>();

        let registry = app.world().resource::<ReflectMapMarkerRegistry>();
        assert!(registry.by_name.contains_key("ActorSlot"));
    }
}
