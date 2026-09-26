/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! ContextFetchers - registerable Systems that find inputs (Contexts) for ActionTemplates.

use bevy::prelude::*;
use bevy::platform::prelude::{String, ToOwned};
use bevy::platform::sync::Arc;
use crate::reinit::{world_structure_changed, CraniumWorldStructureVersion};
use crate::schedule;
use crate::types::{self, ActionContext, AiEntity, CraniumKvMap, CraniumRwLock, PawnEntityRef};
use crate::identifiers::ContextFetcherIdentifier;


/// Convenience type-alias for generic inputs piped into each ContextFetcher. 
/// 
/// You can use it to simply write `fn your_context_fetcher(params: ContextFetcherInputs, your_query:...`
/// instead of having to memorize the specific interface required by the library.
/// 
/// This currently comprises:
/// - Requesting AI - as Entity
/// - Requesting AI's Pawn (controlled Entity) - as Entity
/// 
/// Changes to this interface will be considered semver-breaking once the core lib stabilizes.
/// 
/// Note that ContextFetchers are Plain Old (Read-Only!) Systems, so you can use `Queries`, 
/// `Resources` and all the other Bevy goodness to write your ContextFetcher logic - but you 
/// must also include these inputs as a parameter.
/// 
/// The point of those inputs is to let the World inject metadata about the AI query 
/// into your ContextFetchers so that you can use them in your own logic. 
/// 
/// The key Entities in play in particular are included to enable no-fuss fast 
/// retrieval of data about them in your custom Queries (using `Query::get()`). 
pub type ContextFetcherInputs = bevy::prelude::In<(
    AiEntity, 
    PawnEntityRef,
)>;

/// Convenience type-alias for the output type required from a ContextFetcher System. 
pub type ContextFetcherOutputs = crate::types::CraniumList<ActionContext>;

/// A specialization of Bevy's `System` trait (or more precisely, `ReadOnlySystem`) 
/// that can be used as a Cranium ContextFetcher.
/// 
/// Note that the associated `In` type only adds the restriction that your custom 
/// functions must *at least* accept the metadata piped into them; you can add any 
/// number of Queries, Resource accesses, etc. - as long as they are read-only.
pub trait ContextFetcherSystem: bevy::ecs::system::ReadOnlySystem<
    In = ContextFetcherInputs, 
    Out = ContextFetcherOutputs,
> {}

impl<
    ROS: bevy::ecs::system::ReadOnlySystem<
        In = ContextFetcherInputs, 
        Out = ContextFetcherOutputs,
    >
> ContextFetcherSystem for ROS {}


/// A specialization of Bevy's `IntoSystem` trait that defines any function 
/// that can be turned into a valid Cranium ContextFetcher System.
pub trait IntoContextFetcherSystem<Marker>: IntoSystem<
    ContextFetcherInputs, 
    ContextFetcherOutputs, 
    Marker,
> {}

impl<
    CS: ContextFetcherSystem, 
    Marker, 
    IS: IntoSystem<
        ContextFetcherInputs,
        ContextFetcherOutputs, 
        Marker,
        System = CS
    >
> IntoContextFetcherSystem<Marker> for IS {}


#[derive(Clone)]
pub struct ContextFetcherMappedToSystem {
    pub context_fetcher_system: Result<Arc<CraniumRwLock<dyn ContextFetcherSystem>>, ()>,
}

#[derive(Resource, Default)]
pub struct ContextFetcherKeyToSystemMap {
    pub mapping: CraniumKvMap<
        types::ContextFetcherKey, 
        Arc<CraniumRwLock<dyn ContextFetcherSystem>>
    >,
}


/// Something that allows us to register a ContextFetcher to the World. 
/// 
/// Note that for convenience, the first registration attempt 
/// will initialize *an empty registry* if one does not exist yet, so
/// you don't need to use `app.initialize_resource::<UtilityCurveRegistry>()` 
/// unless you want to be explicit about it.
pub trait AcceptsContextFetcherRegistrations {
    fn register_context_fetcher<
        CS: ContextFetcherSystem, 
        Marker, 
        F: IntoContextFetcherSystem<Marker, System = CS> + 'static,
        IS: Into<String>,
    >(
        &mut self, 
        context_fetcher: F, 
        key: IS,
    ) -> &mut Self;
}

impl AcceptsContextFetcherRegistrations for App {
    fn register_context_fetcher<
        CS: ContextFetcherSystem, 
        Marker, 
        F: IntoContextFetcherSystem<Marker, System = CS> + 'static,
        IS: Into<String>,
    >(
        &mut self, 
        context_fetcher: F, 
        key: IS,
    ) -> &mut Self {
        self.world_mut().register_context_fetcher(context_fetcher, key);
        self
    }
}

impl AcceptsContextFetcherRegistrations for World {
    fn register_context_fetcher<
        CS: ContextFetcherSystem, 
        Marker, 
        F: IntoContextFetcherSystem<Marker, System = CS> + 'static,
        IS: Into<String>,
    >(
        &mut self, 
        context_fetcher: F, 
        key: IS,
    ) -> &mut Self {
        let system = F::into_system(context_fetcher);
        let system_key = ContextFetcherIdentifier::from(key);
        let mut system_registry = self.get_resource_or_init::<ContextFetcherKeyToSystemMap>();            
        
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

pub fn reinit_cf_queries(world: &mut World) {
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

    // Clone handles out so no borrow of world-held resources is live during init.
    let cfs: Vec<_> = world
        .resource::<crate::context_fetchers::ContextFetcherKeyToSystemMap>()
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
        for (key, sys) in cfs {
            match sys.write() {
                Ok(mut s) => { 
                    s.initialize(cell.world_mut()); 
                }

                Err(e) => { 
                    bevy::log::error!("CF {:?} lock poisoned, skipping ({:?})", key, e); 
                }
            }
        }
    }
}

pub struct ContextFetcherPlugin;

impl Plugin for ContextFetcherPlugin {
    fn build(&self, app: &mut App) {
        app
            .init_resource::<ContextFetcherKeyToSystemMap>()
            .add_systems(Startup, reinit_cf_queries.in_set(schedule::CraniumSet::AiInit))
            .add_systems(
                FixedUpdate, 
                reinit_cf_queries
                            .in_set(schedule::CraniumSet::Preflights)
                            .run_if(world_structure_changed)
            )
        ;
    }
}
