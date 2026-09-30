/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! Considerations - registerable Systems that score Actions.

use bevy::platform::prelude::{String, ToOwned};

use bevy::prelude::*;
use bevy::platform::sync::Arc;

use crate::reinit::{world_structure_changed, CraniumWorldStructureVersion};
use crate::schedule;
use crate::types::{self, ActionContextRef, ActionScore, AiEntity, CraniumKvMap, CraniumRwLock, PawnEntityRef};
use crate::identifiers::{ConsiderationIdentifier, CurveIdentifier};

#[cfg(any(feature = "actionset_loader"))]
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, Reflect)]
#[cfg_attr(feature = "actionset_loader", derive(Serialize, Deserialize))]
pub struct ConsiderationData {
    #[cfg_attr(any(feature = "actionset_loader"), serde(rename="consideration"))]
    pub consideration_name: ConsiderationIdentifier,

    #[cfg_attr(any(feature = "actionset_loader"), serde(rename="curve"))]
    pub curve_name: CurveIdentifier,

    pub min: types::ActionScore,
    pub max: types::ActionScore,
}

impl ConsiderationData {
    pub fn new<CNN: Into<ConsiderationIdentifier>, CRN: Into<CurveIdentifier>>(
        consideration_name: CNN,
        curve_name: CRN,
        min: types::ActionScore,
        max: types::ActionScore,
    ) -> Self {
        Self {
            consideration_name: consideration_name.into(),
            curve_name: curve_name.into(),
            min: min, 
            max: max, 
        }
    }
}

/// Convenience type-alias for generic inputs piped into each Consideration. 
/// 
/// You can use it to simply write `fn your_consideration(params: ConsiderationInputs, your_query:...`
/// instead of having to memorize the specific interface required by the library.
/// 
/// This currently comprises:
/// - Requesting AI - as Entity
/// - Requesting AI's Pawn (controlled Entity) - as Entity
/// - The full Context this Consideration is scoring
/// 
/// Changes to this interface will be considered semver-breaking once the core lib stabilizes.
/// 
/// Note that Considerations are Plain Old Systems, so you can use `Query`, `Commands`, 
/// and all the other Bevy goodness to write your Consideration logic - but you must 
/// also include these inputs as a parameter.
/// 
/// The point of those inputs is to let the World inject metadata about the AI query 
/// into your Considerations so that you can use them in your own logic. 
/// 
/// The key Entities in play in particular are included to enable no-fuss fast 
/// retrieval of data about them in your custom Queries (using `Query::get()`). 
pub type ConsiderationInputs = bevy::prelude::In<(
    AiEntity, 
    PawnEntityRef,
    ActionContextRef, 
)>;

/// Convenience type-alias for the output type a Consideration must return.
pub type ConsiderationOutputs = Option<ActionScore>;


/// A specialization of Bevy's `System` trait (or more precisely, `ReadOnlySystem`) 
/// that can be used as a Cranium Consideration.
/// 
/// Note that the associated `In` type only adds the restriction that your custom 
/// functions must *at least* accept the metadata piped into them; you can add any 
/// number of Queries, Resource accesses, etc. - as long as they are read-only.
pub trait ConsiderationSystem: bevy::ecs::system::ReadOnlySystem<
    In = ConsiderationInputs, 
    Out = ConsiderationOutputs
> {}

impl<
    ROS: bevy::ecs::system::ReadOnlySystem<
        In = ConsiderationInputs, 
        Out = ConsiderationOutputs
    >
> ConsiderationSystem for ROS {}


/// A specialization of Bevy's `IntoSystem` trait that defines any function 
/// that can be turned into a valid Cranium Consideration System.
pub trait IntoConsiderationSystem<Marker>: IntoSystem<
    ConsiderationInputs, 
    ConsiderationOutputs, 
    Marker,
> {}

impl<
    Marker, 
    CS: ConsiderationSystem, 
    IS: IntoSystem<
        ConsiderationInputs,
        ConsiderationOutputs, 
        Marker,
        System = CS
    >
> IntoConsiderationSystem<Marker> for IS {}


#[derive(Clone)]
pub struct ConsiderationMappedToSystem {
    pub func_name: ConsiderationIdentifier,

    // NOTE: This is Result<T> as the registry lookup is fallible 
    //       and we will need to propagate these errors later on.
    // pub consideration_systemid: Result<types::ConsiderationSignature, ()>,
    pub consideration_system: Result<Arc<CraniumRwLock<dyn ConsiderationSystem>>, ()>,
    pub curve_name: CurveIdentifier,

    pub min: types::ActionScore,
    pub max: types::ActionScore,
}


#[derive(Resource, Default)]
pub struct ConsiderationKeyToSystemMap {
    pub mapping: CraniumKvMap<
        ConsiderationIdentifier, 
        Arc<CraniumRwLock<dyn ConsiderationSystem>>
    >,
}


/// Something that allows us to register a Consideration to the World. 
/// 
/// Note that for convenience, the first registration attempt 
/// will initialize *an empty registry* if one does not exist yet, so
/// you don't need to use `app.initialize_resource::<UtilityCurveRegistry>()` 
/// unless you want to be explicit about it.
pub trait AcceptsConsiderationRegistrations {
    fn register_consideration<
        CS: ConsiderationSystem, 
        Marker, 
        F: IntoConsiderationSystem<Marker, System = CS> + 'static,
        IS: Into<String>,
    >(
        &mut self, 
        consideration: F, 
        key: IS,
    ) -> &mut Self;
}

impl AcceptsConsiderationRegistrations for App {
    fn register_consideration<
        CS: ConsiderationSystem, 
        Marker, 
        F: IntoConsiderationSystem<Marker, System = CS> + 'static,
        IS: Into<String>,
    >(
        &mut self, 
        consideration: F, 
        key: IS
    ) -> &mut Self {
        self.world_mut().register_consideration(consideration, key);
        self
    }
}

impl AcceptsConsiderationRegistrations for &mut App {
    fn register_consideration<
        CS: ConsiderationSystem, 
        Marker, 
        F: IntoConsiderationSystem<Marker, System = CS> + 'static,
        IS: Into<String>,
    >(
        &mut self, 
        consideration: F, 
        key: IS
    ) -> &mut Self {
        self.world_mut().register_consideration(consideration, key);
        self
    }
}

impl AcceptsConsiderationRegistrations for World {
    fn register_consideration<
        CS: ConsiderationSystem, 
        Marker, 
        F: IntoConsiderationSystem<Marker, System = CS> + 'static,
        IS: Into<String>,
    >(
        &mut self, 
        consideration: F, 
        key: IS
    ) -> &mut Self {
        let system = F::into_system(consideration);
        let system_key = ConsiderationIdentifier::from(key);
        let mut system_registry = self.get_resource_or_init::<ConsiderationKeyToSystemMap>();
        let old = system_registry.mapping.insert(
            system_key.to_owned(), 
            Arc::new(CraniumRwLock::new(
                system
            )));
        match old {
            None => {},
            Some(_) => {
                #[cfg(feature = "logging")]
                bevy::log::warn!(
                    "Detected a key collision for key {:?}. Ejecting previous registration...",
                    system_key
                );
            } 
        }
        self
    }
}

pub fn reinit_consideration_queries(world: &mut World) {
    // Stamp first: if a refresh panics, we don't want to re-enter it forever.
    let generation = world.archetypes().generation();
    match world.get_resource_mut::<CraniumWorldStructureVersion>() {
        Some(mut v) => v.0 = generation,
        None => {
            let mut new_rsc = CraniumWorldStructureVersion::from_world(world);
            new_rsc.0 = generation;
            world.insert_resource(new_rsc);
        }
    }
    
    let cons: Vec<_> = world
        .resource::<crate::considerations::ConsiderationKeyToSystemMap>()
        .mapping
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
    ;

    let cell = world.as_unsafe_world_cell();

    // SAFETY: This is an Exclusive System, so we are the only one with World access, 
    //         and we are the only ones with a lock on the initialized System.
    //         We only really need this to bypass a silly borrow-check on the reference.
    unsafe {
        for (_key, sys) in cons {
            match sys.write() {
                Ok(mut s) => { 
                    s.initialize(cell.world_mut()); 
                }
                
                Err(_e) => { 
                    #[cfg(feature = "logging")]
                    bevy::log::error!("Consideration {:?} lock poisoned, skipping ({:?})", _key, _e); 
                }
            }
        }
    }
}

pub struct ConsiderationPlugin;

impl Plugin for ConsiderationPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ConsiderationKeyToSystemMap>()
            .add_systems(Startup, reinit_consideration_queries.in_set(schedule::CraniumSet::AiInit))
            .add_systems(
                FixedUpdate, 
                reinit_consideration_queries
                    .in_set(schedule::CraniumSet::Preflights)
                    .run_if(world_structure_changed)
            )
        ;
    }
}
