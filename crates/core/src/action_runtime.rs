/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
//! The lifecycle of an AI Action - how it starts, runs, and finishes.

use bevy::prelude::*;

use crate::actions::{Action, ScoredAction};
use crate::action_state::ActionState;
use crate::types;
use crate::events;


// Action Execution
/// A Component that makes this Entity track an AI Action, i.e. store and expose 
/// data about some Action's execution (e.g. 'Is this still running?' or 'How long ago did it start?') 
/// across (potentially) multiple frames and/or serde operations.
/// 
/// This is the 'master'/'root' marker for tracking Actions - all code within this library will assume 
/// anything without this Component does not track anything, and anything that has any of the extension
/// Component also has this Component.
#[derive(Component, Debug)]
pub struct ActionTracker(pub ScoredAction);


/// An 'extension' Component for ActionTracker Bundles.
/// 
/// Adds tracking of the AI Entity that 'owns' the tracked Action. 
/// 
/// Bear in mind that Bevy Entities are just Very Fancy IDs; the 
/// Entity tracked by this Component may no longer be valid - you 
/// should verify it yourself where necessary.
/// 
/// This is primarily intended for tying back Actions to AIs performing them 
/// and for cancelling any Actions without an associated AI owner.
#[derive(Component, Debug)]
pub struct ActionTrackerOwningAI {
    pub owner_ai: crate::types::EntityIdentifier
}


/// An 'extension' Component for ActionTracker Bundles.
/// 
/// Adds Action state tracking to the ActionTracker.
/// In this context, state is roughly the 'lifecycle' of an Action: 
/// Pending -> Running -> Terminal (Successful/Failed/Cancelled/etc.).
/// 
/// This is a very handy piece of information. 
/// You can query it for your Actions to do setup work for Pending or 
/// skip processing for Terminal states, in your UI code to visualize
/// the state of the current behavior or provide a 'Cancel Action' button...
/// You might also (correctly) guess it powers some of the Action lifecycle Events.
#[derive(Component, Debug)]
pub struct ActionTrackerState(pub ActionState);

impl ActionTrackerState {
    pub fn ready() -> Self {
        Self(ActionState::Ready)
    }

    pub fn queued() -> Self {
        Self(ActionState::Queued)
    }

    pub fn get_state(&self) -> &ActionState {
        &self.0
    }

    pub fn set_state(&mut self, state: ActionState) -> &mut Self {
        self.0 = state;
        self
    }
}

/// Helper; wraps how we store time for tracking Action runtime timining.
#[derive(Debug)]
pub enum TimeInstantActionTracker {
    Virtual(core::time::Duration),
    Real(core::time::Duration),
    VirtualAndReal((core::time::Duration, core::time::Duration)),
}

/// An 'extension' Component for ActionTracker Bundles.
/// Adds Action time metadata tracking to the ActionTracker for creation time.
/// 
/// This is separate and different from the START time. 
/// An Action 'Start' is when the execution begins, 'Created' is when the task gets planned.
/// 
/// The primary use-case for this is timeouts and cleanup, especially when combined with other 
/// ActionTracker timers - e.g. if an Action was queued up a year ago and it still hasn't finished, 
/// it is most likely a zombie job that should be terminated. 
/// 
/// However, as with all of these, use it as you wish, it's a building block.
#[derive(Component, Debug)]
pub struct ActionTrackerCreationTimer {
    pub creation_time: TimeInstantActionTracker,
}

/// An 'extension' Component for ActionTracker Bundles. 
/// Adds Action time metadata tracking to the ActionTracker 
/// for Start and End times of the Action's *execution*.
/// 
/// Note that both of these are Option<T>! 
/// 
/// Most obviously, an End time of None means the Action is still pending or running. 
/// You can tell which one it is by the Start time - None means it's pending, i.e. did not start yet.
/// 
/// The primary purpose of this component is for timeouts to kill tasks that
/// got stuck in limbo or did not get cleaned up properly for some reason and the like, 
/// but will almost certainly be handy for UIs and/or Action logic itself as well.
/// 
/// However, as with all of these, use it as you wish, it's a building block.
#[derive(Component, Debug, Default)]
pub struct ActionTrackerRuntimeTimer {
    pub start_time: Option<TimeInstantActionTracker>,
    pub end_time: Option<TimeInstantActionTracker>,
}

/// An 'extension' Component for ActionTracker Bundles. 
/// Adds Action time metadata tracking to the ActionTracker 
/// for tracking when the ActionTracker was last 'ticked' 
/// (processed by a system that implements Actually Doing the Action).
/// 
/// The timer value is Optioned; None indicates the ActionTracker has never been ticked.
/// 
/// This Component makes most sense paired with the ActionTrackerTicks Component 
/// which indicates that this ActionTracker CAN ever be ticked at all.
/// 
/// This is not a hard dependency though. In some cases, it might be useful to add and remove 
/// the 'ticking' to an Action dynamically (e.g. as a LOD optimization), so this link is NOT 
/// enforced by the library on purpose.
/// 
/// You are also free to (ab)use this timer by manually updating it in your Observers whenever 
/// you do anything with the tracked Action in a more event-driven Action implementation as well.
/// 
/// The first purpose of this timer is to detect Action garbage - trackers that 
/// had been created, but are not doing any meaningful work to progress to completion.
/// 
/// The second purpose of this timer is to provide a nice way to get time deltas for 
/// 'sparsely' ticked Actions, e.g. event-driven or when reloaded from a savefile.
/// 
/// However, as with all of these, use it as you wish, it's a building block.
#[derive(Component, Debug, Default)]
pub struct ActionTrackerTickTimer {
    pub last_tick_time: Option<TimeInstantActionTracker>,
}


/// An 'extension' Component for ActionTracker Bundles.
/// 
/// Indicates that this tracker should be processed by a System that   
/// runs some sort of 'tick' logic for the associated Action. 
/// 
/// For example, a MoveTo<pos> Action might move a unit by one grid square, 
/// or one turn's worth of moves, or whatever.
/// 
/// This may be implemented as a function call, a signal emit, both, or whatever else;
/// it's entirely your call as a library user to write your Systems how you want 'em.
/// 
/// This Component is entirely optional - if you don't use it, you can just 
/// catch Events for Action start/end and handle the execution asynchronously 
/// in your game code's 'native' systems (e.g. just set a destination for your 
/// Movement system in an Observer and let it figure out the specifics itself).
#[derive(Component)]
pub struct ActionTrackerTicks;


#[derive(Debug, Clone)]
pub struct ActionTrackerSpawnConfig {
    track_owner_ai: bool, 
    use_ticker: bool,
    use_create_timer: bool,
    use_runtime_timer: bool,
    use_tick_timer: bool,
}

impl ActionTrackerSpawnConfig {
    pub fn builder() -> ActionTrackerSpawnConfigBuilder {
        ActionTrackerSpawnConfigBuilder::default()
    }
}

/// Builder pattern for ActionTrackerSpawnConfig
#[derive(Default, Debug, Clone)]
pub struct ActionTrackerSpawnConfigBuilder {
    track_owner_ai: Option<bool>, 
    use_ticker: Option<bool>,
    use_timers: Option<bool>,
    use_create_timer: Option<bool>,
    use_runtime_timer: Option<bool>,
    use_tick_timer: Option<bool>,
}

impl ActionTrackerSpawnConfigBuilder {
    pub fn build(self) -> ActionTrackerSpawnConfig {
        let track_owner_ai = self.track_owner_ai.unwrap_or(true);
        let use_ticker = self.use_ticker.unwrap_or(false);
        let use_timers = self.use_timers.unwrap_or(false);
        let use_create_timer = self.use_create_timer.unwrap_or(use_timers);
        let use_runtime_timer = self.use_runtime_timer.unwrap_or(use_timers);
        let use_tick_timer = self.use_tick_timer.unwrap_or(use_ticker && use_timers);

        ActionTrackerSpawnConfig {
            track_owner_ai,
            use_ticker,
            use_create_timer,
            use_runtime_timer,
            use_tick_timer,
        }
    }

    pub fn new() -> Self {
        self::default()
    }

    pub fn set_track_owner_ai(mut self, val: bool) -> Self {
        self.track_owner_ai = Some(val); self
    }

    pub fn set_use_ticker(mut self, val: bool) -> Self {
        self.use_ticker = Some(val); self
    }

    pub fn set_use_timers(mut self, val: bool) -> Self {
        self.use_timers = Some(val); self
    }

    pub fn set_use_create_timer(mut self, val: bool) -> Self {
        self.use_create_timer = Some(val); self
    }

    pub fn set_use_runtime_timer(mut self, val: bool) -> Self {
        self.use_runtime_timer = Some(val); self
    }

    pub fn set_use_tick_timer(mut self, val: bool) -> Self {
        self.use_tick_timer = Some(val); self
    }

    /// Creates a new builder using an existing config as a starting point.
    /// This means the values are preconfigured to match the existing config, 
    /// but you can modify them freely before turning this into a new config.
    pub fn from_reference_config(config: &ActionTrackerSpawnConfig) -> Self {
        Self {
            track_owner_ai: Some(config.track_owner_ai),
            use_ticker: Some(config.use_ticker),
            use_create_timer: Some(config.use_create_timer),
            use_runtime_timer: Some(config.use_runtime_timer),
            use_tick_timer: Some(config.use_tick_timer),
            ..Default::default()
        }
    }
}


impl From<ActionTrackerSpawnConfig> for ActionTrackerSpawnConfigBuilder {
    fn from(value: ActionTrackerSpawnConfig) -> Self {
        Self::from_reference_config(&value)
    }
}

impl From<&ActionTrackerSpawnConfig> for ActionTrackerSpawnConfigBuilder {
    fn from(value: &ActionTrackerSpawnConfig) -> Self {
        Self::from_reference_config(value)
    }
}

/// An Event representing some system asking the library to track an Action.
/// You could DIY it, but using this Event should cover typical usecases for ya.
#[derive(EntityEvent)]
pub struct ActionTrackerSpawnRequested {
    /// NOTE: The entity here is intended to be the AIController.
    ///       EntityEvent API sadly doesn't let us rename that for clarity.
    pub entity: types::AiEntity, 
    pub action: ScoredAction, 
    pub tracker_config: Option<ActionTrackerSpawnConfig>,
}

impl ActionTrackerSpawnRequested {
    /// Create a new ActionTracker spawn request.
    pub fn new(entity: Entity, action: ScoredAction, config: Option<ActionTrackerSpawnConfig>) -> Self {
        #[cfg(feature = "logging")]
        bevy::log::debug!(
            "ActionTrackerSpawnRequested::new(): Creating a new ActionTrackerSpawnRequested event for Entity {:?} w/ Action {:?}", 
            entity, action
        );

        Self {
            entity: entity, 
            action: action,
            tracker_config: config,
        }
    }

    /// Create a new ActionTracker spawn request, with whatever defaults the library picked for you. 
    pub fn with_library_defaults(entity: Entity, action: ScoredAction) -> Self {
        Self::new(entity, action, None)
    }

    /// Create a new ActionTracker spawn request with a specified config.
    pub fn with_config(entity: Entity, action: ScoredAction, config: ActionTrackerSpawnConfig) -> Self {
        Self::new(entity, action, Some(config))
    }

    /// Create a new ActionTracker spawn request, allowing the config to be built 
    /// by evaluating a specified callback on the ActionTrackerSpawnConfigBuilder.
    /// 
    /// Kind of like the `Option::or_else()` API and the like, lazy evaluation.
    /// 
    /// Mainly a syntax sugar API so you don't have to create a Builder yourself.
    pub fn with_config_builder(
        entity: Entity, 
        action: ScoredAction, 
        builder: &mut dyn FnMut(ActionTrackerSpawnConfigBuilder) -> Option<ActionTrackerSpawnConfig>,
    ) -> Self {
        Self::new(
            entity, 
            action, 
            builder(ActionTrackerSpawnConfig::builder())
        )
    }
}

/// An Event notifying Observers that a new ActionTracker has been created for an AI.
#[derive(EntityEvent)]
pub struct ActionTrackerSpawnedForTargetAI {
    /// This is the owning AI Entity
    pub entity: Entity,

    /// This is the created ActionTracker Entity
    pub action_tracker: Entity,
}

/// Event handler for spawning ActionTrackers for Actions, 
/// triggered by an ActionTrackerSpawnRequested Event.
pub fn actiontracker_triggered_spawner(
    trigger: On<ActionTrackerSpawnRequested>,
    mut commands: Commands,
    game_timer: Res<Time>,
    real_timer: Res<Time<Real>>,
) {
    let event = trigger.event();
    let owner_ai = event.entity;

    match commands.get_entity(owner_ai) {
        Err(_err) => {
            #[cfg(feature = "logging")]
            bevy::log::warn!(
                "Attempted to spawn an ActionTracker for an AI Entity ({:?}) that no longer exists - {:?}",
                owner_ai, _err
            )
        }

        Ok(mut ai_cmds) => {
            ai_cmds.insert((
                ActionTracker(event.action.clone()),
                ActionTrackerState::ready(),
            ));

            let spawn_config = match &event.tracker_config {
                Some(config) => config,
                None => &ActionTrackerSpawnConfig::builder().build()
            };

            if spawn_config.track_owner_ai {
                ai_cmds.insert(ActionTrackerOwningAI {
                    owner_ai: event.entity.into()
                });
            }

            if spawn_config.use_ticker {
                // Add ticking to this ActionTracker.
                // The Component for this is just a marker, pretty trivial.
                ai_cmds.insert(ActionTrackerTicks);
            }

            // Add timing components.
            // 
            // For now we'll use unwrapped elapsed Durations for this as a standard.
            // Real time in particular may span DAYS for reloads, so wrapping it may cause serious artifacts.
            // Duration is u64-based; you may get issues if you leave your game running for 585 billion years.
            if spawn_config.use_create_timer {
                let virtual_spawn_time = game_timer.elapsed();
                let real_spawn_time = real_timer.elapsed();

                ai_cmds.insert(ActionTrackerCreationTimer {
                    creation_time: TimeInstantActionTracker::VirtualAndReal((virtual_spawn_time, real_spawn_time))
                });
            }

            if spawn_config.use_runtime_timer {
                // The Action hasn't started yet, so they will both be None for now.
                ai_cmds.insert(ActionTrackerRuntimeTimer::default());
            }

            if spawn_config.use_tick_timer {
                // The Action hasn't been ticked, so starts as None.
                ai_cmds.insert(ActionTrackerTickTimer::default());
            }

            // Send a friendly PSA that we have created this Entity for downstream users to hook into.
            ai_cmds.trigger(|atracker| ActionTrackerSpawnedForTargetAI { 
                entity: owner_ai,
                action_tracker: atracker,
            });
        }
    }
}


/// An Event representing some system asking the library to stop tracking an Action.
/// This will usually happen when the Action has reached some terminal state.
/// You could DIY it, but using this Event should cover typical usecases for ya.
#[derive(EntityEvent)]
pub struct ActionTrackerDespawnRequested {
    entity: Entity, 
}

/// A frankly pretty trivial callback that deletes ActionTrackers that were requested to be cleaned up.
/// As each Tracker is its own unique Entity, this will clean up the whole bundle (including optional modules).
/// 
/// If you want to invoke callbacks on success/failure/etc., this should happen BEFORE this event is raised.
pub fn actiontracker_triggered_despawner(
    event: On<ActionTrackerDespawnRequested>,
    mut commands: Commands,
) {
    let _ = commands.get_entity(event.entity).and_then(|mut e| Ok(e.despawn()));
}

pub fn actiontracker_done_cleanup_system(
    query: Query<(Entity, &ActionTracker, &ActionTrackerState)>, 
    mut commands: Commands, 
) {
    // #[cfg(feature = "logging")]
    // bevy::log::debug!("Processing ActionTracker cleanup...");

    for (entity, _tracker, state) in query.iter() {
        let is_done = match state.0 {
            ActionState::Succeeded => true,
            ActionState::Failed => true,
            ActionState::Cancelled => true,
            _ => false,
        };

        #[cfg(feature = "logging")]
        bevy::log::debug!("ActionTrackerCleanup: {:?} is in state {:?} (done: {:?}).", _tracker.0.action.name, state.0, is_done);

        if is_done {
            #[cfg(feature = "logging")]
            bevy::log::info!(
                "ActionTrackerCleanup: Action {:?} of AI {:?} finished, cleaning up the Tracker", 
                _tracker.0.action.name, entity
            );
            commands.trigger(ActionTrackerDespawnRequested {
                entity: entity
            });
        }
    }
}


/// A resource that allows you to specify the global defaults for all ActionTrackers.
/// 
/// If you have a 'house style' for your AI Action implementation, this can save you 
/// a lot of boilerplate.
/// 
/// Note that you can still always override this config for individual exceptional cases.
/// 
/// The provided [`with_config_builder()`] method allows you to conveniently customize the 
/// resource by passing a closure that can mutate the ActionTrackerSpawnConfigBuilder.
#[derive(Default, Resource)]
pub struct UserDefaultActionTrackerSpawnConfig {
    pub config: Option<ActionTrackerSpawnConfig>
}

impl UserDefaultActionTrackerSpawnConfig {
    /// Convenience function for setting up your user default config.
    /// 
    /// Creates a new Config Builder seeded with current config values (or defaults, if none set up), 
    /// then passes it over to the provided FnOnce closure for you to mutate away as you pleas before 
    /// returning the configured result. 
    /// 
    /// The builder then builds and stores the configuration for you.
    pub fn with_config_builder<F: FnOnce(ActionTrackerSpawnConfigBuilder) -> ActionTrackerSpawnConfigBuilder>(
        &mut self, 
        builder: F
    ) -> &mut Self {
        let config_builder = match &self.config {
            None => ActionTrackerSpawnConfigBuilder::new(),
            Some(preexisting) => ActionTrackerSpawnConfigBuilder::from_reference_config(preexisting) 
        };
        let configured = builder(config_builder);
        let built = configured.build();
        self.config = Some(built);
        self
    }
}

/// A batteries-included solution for creating ActionTrackers for your Actions.
/// 
/// Event-driven; responds to AiActionPickedEvent events
pub fn create_tracker_for_picked_action(
    trigger: On<crate::events::AiActionPickedEvent>,
    mut commands: Commands,
    user_default_config_resource: Res<UserDefaultActionTrackerSpawnConfig>,
) {
    let event = trigger.event();

    let action = Action {
        name: event.payload.action_name.clone(),
        action_key: event.payload.action_key.clone(),
        context: event.payload.action_context.clone(),
    };

    let scored_action = ScoredAction {
        action: action,
        score: event.payload.action_score,
    };

    let user_config = user_default_config_resource.config.clone();

    commands.trigger(
        ActionTrackerSpawnRequested::new(
            event.entity,
            scored_action, 
            user_config,
        )
    );
}

/// A System that processes and updates `ActionTrackers` to trigger `Actions`.
/// 
/// This particular implementation uses tick-based [`Action`] processing.
fn tick_based_action_tracker_handler(
    mut query: Query<(
        Entity,
        &ActionTracker, 
        Option<&mut ActionTrackerState>, 
        Option<&mut ActionTrackerTickTimer>
    ), With<ActionTrackerTicks>>,
    mut dispatch_writer: MessageWriter<events::AiActionDispatchToUserCode>,
    game_timer: Res<Time>,
    real_timer: Res<Time<Real>>,
) {
    #[cfg(feature = "logging")]
    bevy::log::debug!(
        "tick_based_action_tracker_handler - Running...", 
    );

    for (ai, tracker, maybe_state, tick_timer) in query.iter_mut() {
        let should_process = maybe_state.as_ref().map(|state| state.0.should_process()).unwrap_or(true);
        
        if !should_process {
            #[cfg(feature = "logging")]
            bevy::log::debug!(
                "tick_based_action_tracker_handler - AI {:?}: Skipping processing for Action(Tracker) {:?} - {:?}", 
                ai, tracker.0.action.name, maybe_state
            );
            continue;
        }

        #[cfg(feature = "logging")]
        bevy::log::debug!(
            "tick_based_action_tracker_handler: processing Action(Tracker) {:?} for {:?} - {:?}", 
            tracker.0.action.name, ai, maybe_state
        );

        if let Some(mut tick_timer_included) = tick_timer {
            let current_time_game = game_timer.elapsed();
            let current_time_real = real_timer.elapsed();

            let new_value = TimeInstantActionTracker::VirtualAndReal((
                current_time_game, current_time_real
            ));

            tick_timer_included.last_tick_time = Some(new_value);
        }

        let message = events::AiActionDispatchToUserCode::new(
            ai, 
            tracker.0.action.action_key.to_owned(), 
            tracker.0.action.name.to_owned(), 
            tracker.0.action.context, 
            tracker.0.score
        );

        dispatch_writer.write(message);
    }
}

/// Sets up the application to use 'ticker'-style Actions backed by ActionTrackers. 
/// 
/// The application will process all running ActionTrackers with 'ticked' Actions 
/// and trigger whichever ActionHandler function you have registered for a registry 
/// key matching the processed Action's own key/ 
/// 
/// These in turn should trigger your own Events you wired your Action implementations 
/// to observe, or some equivalent alternative dispatch implementation method like Messages.
/// 
/// Any picked Actions whose key cannot be resolved to a registered ActionHandler 
/// mapping will be skipped and will not process, nor will any Action in a 
/// terminal state (i.e. - success, failure, cancelled, etc.)
/// 
/// This is a separate plugin, as you may or may not want to use this particular 
/// implementation style for your own applications.
pub struct TickBasedActionTrackerPlugin;

impl Plugin for TickBasedActionTrackerPlugin {
    fn build(&self, app: &mut App) {
        app
        .add_systems(
            FixedPostUpdate, 
            tick_based_action_tracker_handler
        )
        ;
    }
}


// #[cfg(test)]
// mod tests {
//     #[test]
//     fn test_run_action() {}
// }
