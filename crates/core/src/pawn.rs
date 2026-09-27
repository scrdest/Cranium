/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! Pawns - the thing that an AI drives; an NPC, a crowd, a faction, anything.

use bevy::prelude::*;
use crate::{pawn::EntityResolutionError::PawnNotFound, types::{PawnEntity, PawnEntityRef}};

// An AIController on its own is just a jumped-up abstract decision process.
// To get actual NPCs, it needs to 'drive' another Entity - that Entity is the AI's Pawn.
// Note that this can be a classic NPC, but also things like squads, factions, or 'the world' for Director AI.
#[derive(Component, Clone, Default, Reflect)]
#[reflect(Component)]
pub struct Pawn(Option<PawnEntity>);

impl Pawn {
    pub fn new(maybe_pawn: Option<PawnEntity>) -> Self {
        Self(maybe_pawn)
    }

    pub const fn new_empty() -> Self {
        Self(None)
    }

    pub fn new_populated(pawn: PawnEntity) -> Self {
        Self(Some(pawn))
    }

    pub fn as_entity(&self) -> Option<&PawnEntity> {
        match &self.0 {
            None => None,
            Some(pawn_id) => Some(pawn_id)
        }
    }

    pub fn to_entity(self) -> Option<PawnEntity> {
        match self.0 {
            None => None,
            Some(pawn_id) => Some(pawn_id)
        }
    }
}

impl core::borrow::Borrow<PawnEntityRef> for Pawn {
    fn borrow(&self) -> &PawnEntityRef {
        &self.0
    }
}

#[derive(Debug)]
pub enum EntityResolutionError {
    PawnNotFound(String)
}

pub trait AsPawnRef {
    fn try_resolve(&self, world: &World) -> Result<Entity, EntityResolutionError>;
}

impl AsPawnRef for Entity {
    fn try_resolve(&self, world: &World) -> Result<Entity, EntityResolutionError> {
        world.get_entity(*self).map_or_else(
            |err| {
                Err(PawnNotFound(err.to_string()))
            }, 
            |ent| {
                Ok(ent.id())
            }
        )
    }
}

pub enum PawnRef {
    BevyEntity(Entity),
    // TODO: A Cranium-native ID-to-Entity Registry
    // CraniumPawnId(PawnId)
    Custom(Box<dyn AsPawnRef>)
}

impl PawnRef {
    pub fn try_as_entity(&self, world: &World) -> Result<Entity, EntityResolutionError> {
        match self {
            PawnRef::BevyEntity(e) => e.try_resolve(world),
            PawnRef::Custom(dynamic) => dynamic.try_resolve(world)
        }
    }
}
