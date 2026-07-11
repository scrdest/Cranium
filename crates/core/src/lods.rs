/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
//! AI LOD (Level of Detail) system for optimizing processing in complex worlds.
//! 
//! AI logic tends to be a 'hot and heavy' System - resource-heavy and frequently running. 
//! This creates a scaling problem, creating a cap on the number of AIs that can be active in the world.
//! 
//! While we have the power of Rust, ECS, Utility AI, and (de rigeur) anime on our side 
//! to make AI logic as fast as it can possibly get, this only delays the inevitable.
//! 
//! If your app consists of relatively small and self-contained scenes, this may not be a problem, but 
//! we mustn't be afraid to dream a little bit bigger and think of crowds and open worlds here.
//! 
//! The capabilities provided by this module are one piece of the puzzle and a classic solution 
//! for these kinds of scaling problems (1).
//! 
//! Rather than processing the same AI logic all the time, we maintain a separate system (or a whole 
//! suite of them) that decides how 'active' each AI controller should be and update a marker Component. 
//! 
//! This can range from 'ignore it altogether', through 'use cheaper, low-fi versions of the logic', 
//! all the way to 'full steam ahead' for NPCs near the player and even 'elevated' processing for 
//! special situations (e.g. a boss NPC during a bossfight scene vs normal wandering around nearby).
//! 
//! In practice, this is implemented as a simple value and a pair of attributes on the ActionTemplate, 
//! `min_lod` and `max_lod`. A Template is skipped if its AI's current LOD is not between those two values.
//! 
//! The exact logic of the LOD-setting systems are left up to the user; the library provides the levels 
//! and an integration of the LODs into the core Utility AI engine, since user code cannot hook into it. 
//! 
//! By default all AIs are running on LOD_NORMAL at all times. This is to ensure this feature 
//! works entirely as an opt-in solution for those applications that actually need it and don't 
//! slow down your development in applications that do not call for it.
//! 
//! If you do not specify otherwise, the min LOD for any Action is `LOD_NORMAL` and the max LOD is `LOD_MINIMAL`.
//! 
//! This means you can just use `LOD_NORMAL` and `LOD_INACTIVE` as your only two levels 
//! if all you're interested in is enabling and disabling AI processing with no further 
//! granularity and spare your AI designers from specifying min/max LODs in ActionSets.
//! 
//! (1) - the other piece, also available via this library, is grouping - AIs do not have to correspond 
//! to NPCs 1:1, a whole crowd can share one collective 'brain' that controls the overall 'flow'.

use bevy::ecs::component::Component;
use bevy::prelude::ReflectComponent;

use crate::types::AiLodLevelPrimitive;

/* =====    Constant values for nice static reference    ===== */
// Note that there is headroom between the middle values to introduce more granular
// subdivisions between current levels if it becomes obvious we need 'em in the future.

/// Absolutely everything that runs at full precision should run, nothing is approximated for this AI.
pub const LOD_ELEVATED: AiLodLevelPrimitive = AiLodLevelPrimitive::MIN;   // i.e. 0u8

/// The usual mode of operation of an NPC near the player.  
pub const LOD_NORMAL: AiLodLevelPrimitive = AiLodLevelPrimitive::MIN + 8; // i.e. 8u8

/// The lowest level of detail possible before the AI doesn't run at all. 
pub const LOD_MINIMAL: AiLodLevelPrimitive = AiLodLevelPrimitive::MAX - 1;   // i.e. 254u8

/// Indicates the AI is entirely disabled; the library is free to ignore it entirely 
/// until the LOD changes to a lower value (= higher level of processing detail).
pub const LOD_INACTIVE: AiLodLevelPrimitive = AiLodLevelPrimitive::MAX;   // i.e. 255u8


#[derive(Clone, Copy, bevy::reflect::Reflect, Debug)]
pub struct AILevelOfDetailValue(AiLodLevelPrimitive);

impl AILevelOfDetailValue {
    pub fn new(level: AiLodLevelPrimitive) -> Self {
        Self(level)
    }

    pub const fn new_const<const LVL: AiLodLevelPrimitive>() -> Self {
        Self(LVL)
    }

    pub(crate) fn to_primitive(self) -> AiLodLevelPrimitive {
        self.0
    }

    pub fn is_inactive(&self) -> bool {
        self.0 == LOD_INACTIVE
    }
}

impl Default for AILevelOfDetailValue {
    fn default() -> Self {
        Self(LOD_NORMAL)
    }
}

#[derive(Component, Default, bevy::reflect::Reflect, Clone)]
#[reflect(Component)]
pub struct AILevelOfDetail {
    lod: AILevelOfDetailValue
}

impl AILevelOfDetail {
    pub fn new(level: AILevelOfDetailValue) -> Self {
        Self { lod: level }
    }

    pub fn new_from_value(level: AiLodLevelPrimitive) -> Self {
        Self { lod: AILevelOfDetailValue::new(level) }
    }

    pub const fn new_const_from_value<const LVL: AiLodLevelPrimitive>() -> Self {
        Self { lod: AILevelOfDetailValue::new_const::<LVL>() }
    }

    pub fn get_current_lod(&self) -> AILevelOfDetailValue {
        self.lod
    }
}
