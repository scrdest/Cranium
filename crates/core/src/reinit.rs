use bevy::prelude::*;
use bevy::ecs::archetype::ArchetypeGeneration; 

/// The World's archetype generation as of the last registry refresh.
/// One value replaces `ShouldReinitCfQueries` + `ShouldReinitConsiderationQueries`:
/// staleness is a property of the WORLD's structure, not of a decision having run.
#[derive(Resource, Debug, Clone, Copy)]
pub struct CraniumWorldStructureVersion(pub ArchetypeGeneration);

impl FromWorld for CraniumWorldStructureVersion {
    fn from_world(world: &mut World) -> Self {
        Self(world.archetypes().generation())
    }
}

/// Run condition: archetypes were created since the last refresh.
pub fn world_structure_changed(world: &World) -> bool {
    world.archetypes().generation() != world.resource::<CraniumWorldStructureVersion>().0
}

pub fn reinit_registries(world: &mut World) {
    // Stamp first: if a refresh panics, we don't want to re-enter it forever.
    let generation = world.archetypes().generation();
    world.resource_mut::<CraniumWorldStructureVersion>().0 = generation;

    // Clone handles out so no borrow of world-held resources is live during init.
    let cfs: Vec<_> = world
        .resource::<crate::context_fetchers::ContextFetcherKeyToSystemMap>()
        .mapping
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
    ;
    
    let cons: Vec<_> = world
        .resource::<crate::considerations::ConsiderationKeyToSystemMap>()
        .mapping
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
    ;

    let cell = world.as_unsafe_world_cell();

    // SAFETY: exclusive system - sole World access, and each System is locked
    // exclusively before we initialize it.
    unsafe {
        for (_key, sys) in cfs {
            match sys.write() {
                Ok(mut s) => { 
                    s.initialize(cell.world_mut()); 
                }

                Err(_e) => { 
                    #[cfg(feature = "logging")]
                    bevy::log::error!("CF {:?} lock poisoned, skipping ({:?})", _key, _e); 
                }
            }
        }

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

