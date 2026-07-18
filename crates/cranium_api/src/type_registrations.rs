/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
//! This module handles AppTypeRegistry type registrations for key Component 
//! types that are needed to support key Cranium functionalities.
//! 
//! Cranium's standalone-mode API allows the client to request 
//! spawns of ECS Entities with arbitrary Components attached. 
//! 
//! However, to be able to deserialize those requests, 
//! the deserializer requires those Component types to 
//! be registered in the AppTypeRegistry (and implement Reflect).
//! 
//! So, everything that is registered here can be spawned and attached to an Entity.

use cranium_core::bevy::prelude::*;

use cranium_core::ai::AIController;
use cranium_core::lods::AILevelOfDetail;
use cranium_core::pawn::Pawn;
use cranium_core::smart_object::SmartObjects;

pub struct CraniumApiTypesRegistrationPlugin;


/// A basic 2D vector struct, mainly used to showcase a non-AI dyn-spawnable Component
#[derive(Component, Reflect, serde::Serialize, serde::Deserialize)]
#[reflect(Component)]
pub struct IntPosition2d {
    x: i64,
    y: i64,
}


impl Plugin for CraniumApiTypesRegistrationPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AIController>();
        app.register_type::<AILevelOfDetail>();
        app.register_type::<Pawn>();
        app.register_type::<SmartObjects>();
        app.register_type::<IntPosition2d>();
    }
}
