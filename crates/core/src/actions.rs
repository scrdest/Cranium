/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
//! Actions and ActionTemplates. Action = ActionTemplate + Context (from a ContextFetcher).
//! 
//! An Action is the ultimate output of the AI engine - it is largely what it sounds like, 
//! a generic behavior of some sort, for example moving, attacking, using items, etc., 
//! possibly taking multiple ticks to conclude (as opposed to a one-off trigger only).
//! 
//! The first caveat is that Actions are independent of their implementation - AI code cares 
//! not about how you implement your movement or combat systems, an Action is merely a request 
//! to run some code that will implement it properly (with hooks to signal when it's done running).
//! 
//! The second caveat is that we are not necessarily only talking about individual NPCs!
//!  
//! A single AI can be the 'brain' of a single NPC, but we can also have 'Commander' AIs 
//! driving whole teams or crowds, 'Director' AIs whose target (Pawn) is 'the game world'
//! (e.g. running spawners, or coordinating other AIs goals to drive story progression), 
//! and conversely, a single NPC or Faction can be run by multiple AIs working together.
//! 
//! An Action is composed of two parts: an ActionTemplate and an ActionContext. 
//! 
//! You can think of that as 'function' and 'parameters', more or less - the 
//! Template represents a capability, e.g. the ability to pick up items, while the 
//! Context represents what we're doing it *to* (e.g. which item to pick up). 
//! 
//! An Action will usually have multiple possible available Contexts to choose from. 
//! 
//! This is the core problem AI solves - given all ActionTemplates and all Contexts 
//! available for them at a given moment, which combination to choose for execution?
use bevy::prelude::*;

#[cfg(any(feature = "actionset_loader"))]
use serde::{Serialize, Deserialize};

use crate::considerations::ConsiderationData;
use crate::types::{self, ActionContextRef, CraniumList, CraniumKvMap};
use crate::identifiers::{ContextFetcherIdentifier};

pub type ActionContext = Entity;
pub type ActionKey = String;

/// An Action is effectively an ActionTemplate + a selected ActionContext. 
#[derive(Clone, Reflect, Debug)]
pub struct Action {
    pub name: String,
    pub context: ActionContextRef,
    pub action_key: ActionKey,
}

#[derive(Clone, Reflect, Debug)]
pub struct ScoredAction {
    /// 
    pub action: Action,
    pub score: types::ActionScore,
}


/// An ActionTemplate is a 'partial' Action (in the sense of a partial function).
/// It represents an abstract activity an AI may undertake without a specific target.
/// 
/// For example, OpenDoor is an ActionTemplate - it becomes an Action when we specify 
/// WHICH door to open, which we'll refer to by the quasi-generic syntax `OpenDoor<SomeDoor>`. 
/// 
/// The square-bracketed value(s) are what the library calls an ActionContext.
/// 
/// In total, an ActionTemplate is: 
/// 1) a String key identifying the Action executor e.g. 'OpenDoorHandler'.
/// 2) a String key identifying a System that returns possible Contexts (e.g. 'GetAdjacentDoors').
/// 3) a sequence of Consideration System String keys that will be used to score all of these (e.g. ['DistanceToPawn']).
/// 4) a Priority multiplier for the final score to make certain activities intrinsically higher priority (e.g. 1.5).
#[derive(Debug, Clone, Reflect)]
#[cfg_attr(any(feature = "actionset_loader"), derive(Serialize, Deserialize))]
pub struct ActionTemplate {
    // 
    // name = identifier. Two ActionTemplates may share the same function (as an implementation detail), 
    //                    but represent very different logical activities. This helps AI designers not go mad.

    /// Human-readable identifier of the Action; as a general rule, should be 
    pub name: String, 

    /// 
    #[cfg_attr(any(feature = "actionset_loader"), serde(rename="context_fetcher"))]
    pub context_fetcher_name: ContextFetcherIdentifier,
    pub considerations: CraniumList<ConsiderationData>,
    pub priority: types::ActionScore,
    pub action_key: ActionKey,
    // AI LODs: 
    pub lod_min: Option<types::AiLodLevelPrimitive>,
    pub lod_max: Option<types::AiLodLevelPrimitive>,
}

impl ActionTemplate {
    pub fn new<
        INS: Into<String>,
        CFN: Into<ContextFetcherIdentifier>,
        IAK: Into<String>,
    >(
        name: INS,
        context_fetcher_name: CFN,
        considerations: CraniumList<ConsiderationData>,
        priority: types::ActionScore, 
        action_key: IAK, 
        lod_min: Option<types::AiLodLevelPrimitive>,
        lod_max: Option<types::AiLodLevelPrimitive>,
    ) -> Self {
        Self {
            name: name.into(),
            context_fetcher_name: context_fetcher_name.into(),
            considerations: considerations,
            priority: priority,
            action_key: action_key.into(),
            lod_min: lod_min,
            lod_max: lod_max,
        }
    }

    /// Checks if this template should be processed at a given LOD.
    pub fn is_within_lod_range(&self, lod: &Option<crate::lods::AILevelOfDetailValue>) -> bool {
        let qry_lod = lod.map(|lv| lv.to_primitive()).unwrap_or(crate::lods::LOD_NORMAL);
        let min = self.lod_min.unwrap_or(crate::lods::LOD_NORMAL);
        let max = self.lod_max.unwrap_or(crate::lods::LOD_MINIMAL);
        qry_lod >= min && qry_lod <= max
    }
}

impl core::hash::Hash for ActionTemplate {
    /// ActionTemplates are equal if their Name and ActionKey are equal.
    /// Both are hashable, so we'll use them for hash keys as well.
    /// 
    /// Note that you may need to look up the canonical values of other fields from an Asset or w/e.
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.action_key.hash(state);
    }
}

impl PartialEq for ActionTemplate {
    /// ActionTemplates are equal if their Name and ActionKey are equal.
    /// Really we only need Names mostly, the ActionKey is just a safeguard on accidental clashes.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.action_key == other.action_key
    }
}

impl Eq for ActionTemplate {}

/// Convenience type-alias for the input type required from an ActionHandler. 
pub type ActionHandlerInputs = (
    types::AiEntity,
    types::PawnEntityRef,
    types::ActionContextRef,
);

/// Convenience type-alias for the output type required from an ActionHandler. 
pub type ActionHandlerOutputs = ();

/// Convenience type-alias for register-able ActionHandlers
pub type ActionHandlerFn = dyn Send + Sync + FnMut(ActionHandlerInputs, Commands) -> ();

/// A 'guardrail' trait for Events that trigger your Action logic. 
/// 
/// This trait is not used as an actual bound in any Cranium code; 
/// the point of it is to give you an interface which, once implemented, 
/// will ensure you've got the basics covered for working with Actions 
/// and a solid base for building simple ActionHandler functions quickly.
pub trait ActionTriggerEvent: Event {
    /// The event should be buildable from just the standardized inputs.
    fn build_from_action_handler_inputs(inputs: ActionHandlerInputs) -> Self;

    /// The event should be buildable and triggerable in an ActionHandler function. 
    /// 
    /// The simplest impl is `commands.trigger(Self::build_from_action_handler_inputs(inputs))`, 
    /// but there's some extra bounds on Triggers that made it impossible to provide out of the box. 
    fn trigger_from_action_handler(inputs: ActionHandlerInputs, commands: Commands);
}

/// A thin wrapper over an ActionHandler function.
pub struct ActionPickCallback(Box<ActionHandlerFn>);

impl ActionPickCallback {
    pub fn new<F: Send + Sync + FnMut(ActionHandlerInputs, Commands) -> () + 'static>(func: F) -> Self {
        Self(Box::new(func))
    }

    pub fn call(&mut self, inputs: ActionHandlerInputs, commands: Commands) {
        self.0(inputs, commands)
    }
}

impl<F: 'static + Send + Sync + FnMut(ActionHandlerInputs, Commands)> From<F> for ActionPickCallback {
    fn from(value: F) -> Self {
        Self::new(value)
    }
}

#[derive(Default, Resource)]
pub struct ActionHandlerKeyToSystemMap {
    pub mapping: CraniumKvMap<
        types::ActionKey, 
        ActionPickCallback,
    >
}


/// Something that allows us to register a ActionHandler to the World. 
/// 
/// Note that for convenience, the first registration attempt 
/// will initialize *an empty registry* if one does not exist yet, so
/// you don't need to use `app.initialize_resource::<UtilityCurveRegistry>()` 
/// unless you want to be explicit about it.
pub trait AcceptsActionHandlerRegistrations {
    fn register_action_handler<
        ITF: Into<ActionPickCallback>, 
        IS: Into<String>,
    >(
        &mut self, 
        trigger_fn: ITF, 
        key: IS,
    ) -> &mut Self;
}

impl AcceptsActionHandlerRegistrations for App {
    fn register_action_handler<
        ITF: Into<ActionPickCallback>, 
        IS: Into<String>,
    >(
        &mut self, 
        trigger_fn: ITF, 
        key: IS,
    ) -> &mut Self{
        self.world_mut().register_action_handler(trigger_fn, key);
        self
    }
}

impl AcceptsActionHandlerRegistrations for World {
    fn register_action_handler<
        ITF: Into<ActionPickCallback>, 
        IS: Into<String>,
    >(
        &mut self, 
        trigger_fn: ITF, 
        key: IS,
    ) -> &mut Self {
        let system_key: ActionKey = key.into();
        let callback = trigger_fn.into();
        let cell = self.as_unsafe_world_cell();

        let mut system_registry = match unsafe { cell.world_mut() }.get_non_send_resource_mut::<ActionHandlerKeyToSystemMap>() {
            Some(registry) => registry,
            None => {
                unsafe { cell.world_mut() }.init_resource::<ActionHandlerKeyToSystemMap>();
                unsafe { cell.world_mut() }.get_resource_mut::<ActionHandlerKeyToSystemMap>().unwrap()
            }
        };

        let old = system_registry.mapping.insert(
            system_key.to_owned(), 
            callback
        );
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

pub struct ActionHandlerPlugin;

impl Plugin for ActionHandlerPlugin {
    fn build(&self, app: &mut App) {
        app
            // Technically unnecessary, but will give users saner error messages if we pre-initialize:
            .init_resource::<ActionHandlerKeyToSystemMap>()
        ;
    }
}
