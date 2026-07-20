/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! The values used by the Action Runtime to track the state of AI Actions.

use bevy::prelude::*;
use bevy::{platform::collections::Equivalent, reflect::Reflect};

use crate::{types, action_runtime::ActionTrackerState};

#[cfg(any(feature = "actionset_loader"))]
use serde::{Deserialize, Serialize};

/// A lifecycle marker for ActionTrackers to indicate what the status of the tracked Action is. 
/// 
/// This is a state machine, more or less, with three layers:
/// 1) Initial
/// 2) Progressed
/// 3) Terminal
/// 
/// *As a general rule*, the progression through those layers should be non-increasing, i.e.:
/// - Terminal States should never change at all once reached, 
/// - Progressed states should only become Terminal or different Progressed States, and
/// - Initial states can become any other State.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Reflect)]
#[cfg_attr(any(feature = "actionset_loader"), derive(Serialize, Deserialize))]
pub enum ActionState {
    // Note that we are NOT implementing Default for this on purpose; 
    // the default state is domain-specific (though probably just Ready most of the time).

    /// Initial state. Planned but not started and cannot start yet - waiting on something (likely other Actions).
    Queued, 
    /// Initial state. Can start now, hadn't done literally anything yet though.
    Ready, 
    
    /// Progressed state. Started but didn't finish yet, will continue.
    Running, 
    /// Progressed state. Started and can continue, but has been put on hold for now.
    Paused, 
    
    /// Terminal state. Did all it was supposed to and is no longer needed, yaaay.
    Succeeded, 
    
    /// Terminal state. 
    /// We gave up due to getting stuck/timeout/etc., naaay. 
    /// Generally implies the action should not be retried as-is 
    /// and may trigger failure callbacks.
    Failed, 
    
    /// Terminal state. 
    /// We gave up because of an 'external' decision that we should, 
    /// and NOT because something was wrong with the execution. 
    /// The Action may well be still valid, we just stopped pursuing it. 
    /// You can think of it as 'Paused, but forever'. 
    Cancelled, 
}

impl ActionState {
    /// A shorthand for checking if an Action is in one of the Initial states (e.g. Ready).
    pub fn is_initial(&self) -> bool {
        match self {
            Self::Queued => true,
            Self::Ready => true,
            _ => false,
        }
    }

    /// A shorthand for checking if an Action is in one of the Progressed states (e.g. Running).
    pub fn is_progressed(&self) -> bool {
        match self {
            Self::Running => true,
            Self::Paused => true,
            _ => false,
        }
    }

    /// A shorthand for checking if an Action is in one of the Terminal states (e.g. Succeeded).
    pub fn is_terminal(&self) -> bool {
        match self {
            Self::Succeeded => true,
            Self::Failed => true,
            Self::Cancelled => true,
            _ => false,
        }
    }

    /// A shorthand for checking if an Action should be processed/'ticked'. If false, we can skip it.
    pub fn should_process(&self) -> bool {
        match self {
            Self::Ready => true,
            Self::Running => true,
            _ => false,
        }
    }
}

impl Equivalent<ActionState> for &ActionState {
    fn equivalent(&self, key: &ActionState) -> bool {
        self == &key
    }
}


/// 
#[derive(Message)]
pub struct AiActionStateChangeRequest {
    pub entity: Entity,
    pub action: types::ActionKey,
    pub to_state: crate::action_state::ActionState,
}


/// Signals that an Action has transitioned between two states: from_state -> to_state.
#[derive(EntityEvent)]
pub struct AiActionStateChange {
    pub entity: Entity,
    pub action: types::ActionKey,
    pub from_state: Option<crate::action_state::ActionState>,
    pub to_state: crate::action_state::ActionState,
}

/// A System that processes all pending AiActionStateChangeRequest and applies them. 
/// Can be scheduled as a System or (via `action_state_update_handler_observer()`) as an Observer.
pub fn action_state_update_handler(
    mut request_reader: MessageReader<AiActionStateChangeRequest>,
    mut tracker_state_qry: Query<&mut ActionTrackerState>,
    mut commands: Commands,
) {
    request_reader.read().for_each(|msg| {
        let maybe_tracker_state = tracker_state_qry.get_mut(msg.entity);

        match maybe_tracker_state {
            Err(err) => {
                bevy::log::debug!("{:?}: ActionTracker does not exist: {:?}", &msg.action, err);
                match commands.get_entity(msg.entity) {
                    Err(err) => {
                        #[cfg(feature = "logging")]
                        bevy::log::error!("{:?}: AI {:?} does not exist??? - {:?}", &msg.action, msg.entity, err);
                    }
                    Ok(mut cmds) => {
                        #[cfg(feature = "logging")]
                        bevy::log::debug!("{:?}: Inserting new ActionState for AI {:?} - {:?}", &msg.action, msg.entity, &msg.to_state);
                        cmds.trigger(|ent| AiActionStateChange {
                            action: msg.action.clone(),
                            entity: ent,
                            from_state: None, 
                            to_state: msg.to_state.clone(),
                        });
                        cmds.insert(ActionTrackerState(msg.to_state));
                    }
                }
            }
            Ok(mut state) => { 
                #[cfg(feature = "logging")]
                bevy::log::debug!("example_action for AI {:?}: Updating the state to new value {:?}", msg.entity, msg.to_state);
                let current = state.get_state().clone();
                commands.trigger(AiActionStateChange {
                    action: msg.action.clone(),
                    entity: msg.entity,
                    from_state: Some(current), 
                    to_state: msg.to_state.clone(),
                });
                state.set_state(msg.to_state);
            },
        }
    });
}


/// An event that 
#[derive(Event)]
pub struct ProcessActionStateUpdatesSignal;


/// An Observer wrapper for `action_state_update_handler()`, triggered by ProcessActionStateUpdatesSignal. 
/// Can be used as a 'sparse' replacement for normal Schedule-based processing or in parallel to it as a 
/// way to force a flush of the buffered AiActionStateChangeRequests.
pub fn action_state_update_handler_observer(
    _trigger: On<ProcessActionStateUpdatesSignal>,
    request_reader: MessageReader<AiActionStateChangeRequest>,
    tracker_state_qry: Query<&mut ActionTrackerState>,
    commands: Commands,
) {
    action_state_update_handler(request_reader, tracker_state_qry, commands);
}


pub struct ActionStateUpdatesPlugin;

impl Plugin for ActionStateUpdatesPlugin {
    fn build(&self, app: &mut App) {
        app
        .add_message::<AiActionStateChangeRequest>()
        .add_observer(action_state_update_handler_observer)
        .add_systems(FixedUpdate, crate::action_state::action_state_update_handler)
        ;
    }
}
