/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! This is a relatively thin module with a simple job: definine the SystemStages
//! for Cranium's AI systems to allow downstream users to order their own Stages 
//! relative to Cranium's own processing.
 
use bevy::prelude::*;

/// Opaque mount point for Startup-stage AI stuff. Hosts order THIS, never the internals.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CraniumAiStartupStage;

/// Opaque mount point for FixedUpdate-stage AI stuff. Hosts order THIS, never the internals.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CraniumAiUpdateStage;

/// Cranium's private ordering. Not part of the host contract.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CraniumSet {
    /// Any app-level init stuff done by Cranium. 
    AiInit,

    /// Any setup required for AI systems to run.
    Preflights,

    /// Where decision requests are raised.
    Request,

    /// Where the decisions actually get made.
    Process,

    /// ActionHandlers fire; picked actions become host intents.
    Dispatch,

    /// Tracker ticks, state updates, cleanup.
    Maintain,
}

pub struct CraniumScheduleStagesPlugin;

impl Plugin for CraniumScheduleStagesPlugin {
    fn build(&self, app: &mut App) {
        app
        
        // Startup
        .configure_sets(
            Startup, 
            (
                CraniumSet::AiInit,
            )
            .chain()
            .in_set(CraniumAiStartupStage)
        )

        // FixedUpdate
        .configure_sets(
            FixedUpdate, 
            (
                CraniumSet::Preflights,
                CraniumSet::Request,
                CraniumSet::Process,
                CraniumSet::Dispatch,
                CraniumSet::Maintain,
            )
            .chain()
            .in_set(CraniumAiUpdateStage)
        )
        ;
    }
}
