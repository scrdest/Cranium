/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! Assorted Event types used by Cranium.

use bevy::{ecs::{entity::{EntityHashMap, EntityHashSet}, message::MessageId}, prelude::*};
use crate::{actions::ActionContext, types::{self, RequestKey}};

/// The core details about the action selection decision. 
/// 
/// This primarily exists to enable both Message- and Event-based implementations 
/// for signalling action-picking outcomes.
#[derive(Debug, Clone)]
pub struct AiActionPickedPayload {
    /// Identifier for the handling event (e.g. "GoTo"). 
    /// This is effectively a link to the *implementation* of the action. 
    pub action_key: crate::types::ActionKey,

    /// Human-readable primary identifier; one action_key may handle distinct action_names 
    /// (e.g. action_key "GoTo" may cover action_names "Walk", "Run", "Flee", etc.).
    /// In other words, this is what this action represents *semantically*, and is less likely
    /// to change for technical purposes.
    pub action_name: String,

    /// The Context of the Action, i.e. the static input(s) we scored against.
    pub action_context: crate::types::ActionContextRef,

    /// The Utility score; this is so that we can decide whether to possibly 
    /// override this with a higher-priority Action later on.
    pub action_score: crate::types::ActionScore,

    /// Identifier of the request that has originally triggered this decision. 
    /// Used to tie the reponse back to the request that led to it.
    pub request_key: Option<RequestKey>,
}

/// An Event that signals the decision engine picked the new best Action
/// for a specific AI Entity and provides details about it (abstract ID, 
/// context, etc.).
/// 
/// Primarily expected to be raised by the decision_process() System 
/// and listened to by consumers for remapping into more Action-specific logic
/// (e.g. raising an Event for a *specific* Action implementation).
/// 
/// This and AiActionPickedMessage are effectively siblings - this is the Observer version, 
/// for use-cases that favor sparse, broadcast processing of decisions.
#[derive(EntityEvent, Debug)]
pub struct AiActionPickedEvent {
    /// The AI that picked this Action for execution. 
    pub entity: Entity,

    /// The actual details of the decisions.
    pub payload: AiActionPickedPayload
}

impl AiActionPickedEvent {
    pub fn new(
        ai_owner: Entity,
        action_key: crate::types::ActionKey,
        action_name: String,
        action_context: ActionContext,
        action_score: crate::types::ActionScore,
        request_key: Option<RequestKey>,
    ) -> Self {
        
        #[cfg(feature = "logging")]
        bevy::log::debug!(
            "Creating a new AiActionPickedEvent event for {:?} with key {:?} ({:?})",
            ai_owner,
            action_key,
            action_name
        );

        let wrapped_ctx = action_context;

        let payload = AiActionPickedPayload {
            action_key: action_key,
            action_name: action_name,
            action_context: wrapped_ctx,
            action_score: action_score,
            request_key: request_key,
        };

        Self {
            entity: ai_owner,
            payload
        }
    }
}

/// A Message that signals the decision engine picked the new best Action
/// for a specific AI Entity and provides details about it (abstract ID, 
/// context, etc.).
/// 
/// Primarily expected to be sent by the decision_process() System 
/// and listened to by consumers for remapping into more Action-specific logic
/// (e.g. raising an Event for a *specific* Action implementation).
/// 
/// This and AiActionPickedEvent are effectively siblings - this is the Message version, 
/// for use-cases that favor batch processing of decisions.
#[derive(Message, Debug, Clone)]
pub struct AiActionPickedMessage {
    /// The AI that picked this Action for execution. 
    pub entity: Entity,

    /// The actual details of the decisions.
    pub payload: AiActionPickedPayload
}

impl AiActionPickedMessage {
    pub fn new(
        ai_owner: Entity,
        action_key: crate::types::ActionKey,
        action_name: String,
        action_context: ActionContext,
        action_score: crate::types::ActionScore,
        request_key: Option<RequestKey>,
    ) -> Self {
        
        #[cfg(feature = "logging")]
        bevy::log::debug!(
            "Creating a new AiActionPickedEvent event for {:?} with key {:?} ({:?})",
            ai_owner,
            action_key,
            action_name
        );

        let wrapped_ctx = action_context;

        let payload = AiActionPickedPayload {
            action_key: action_key,
            action_name: action_name,
            action_context: wrapped_ctx,
            action_score: action_score,
            request_key: request_key,
        };

        Self {
            entity: ai_owner,
            payload
        }
    }
}

/// Supporting Event for triggering a decision_process() for an AI.
/// 
/// Should generally NOT be raised more than once per Entity per tick 
/// or you are likely running the same calculation multiple times.
/// 
/// Historically, this has been the direct trigger; since Cranium v0.4 onwards, 
/// this is only a convenience method for writing an AiDecisionRequestedMessage 
/// to be processed at the next tick of the usual, predictable AI schedule stage.
#[derive(EntityEvent)]
pub struct AiDecisionRequested {
    pub entity: types::AiEntity,
    pub request_key: Option<RequestKey>,
    pub smart_objects: Option<crate::types::SmartObjects>,
    pub pawn: Option<crate::types::PawnEntity>,
}


/// A Message that triggers a decision_process() for an AI.
/// 
/// This is the real core trigger for decision_process() to fire.
/// 
/// The messages are processed at most once per FixedUpdate tick, so Cranium reserves the 
/// right to deduplicate requests based on `(Entity, tick)` for at-least-once-style processing. 
/// 
/// From a different angle: within the same tick, a decision request is idempotent.
#[derive(Message, Clone)]
pub struct AiDecisionRequestedMessage {
    /// The AI that should make a decision
    pub entity: types::AiEntity,
    
    /// Soft-required: a list of SmartObjects to source Actions from. 
    /// This CAN be None, but that will almost certainly lead to an empty decision 
    /// (which can be fine for testing or if your application just doesn't check on its side it has any).
    pub smart_objects: Option<types::SmartObjects>,

    /// Optional: Correlation key
    pub request_key: Option<RequestKey>,

    /// Optional: The Pawn this request is for. 
    pub pawn: Option<types::PawnEntity>,
}

impl AiDecisionRequestedMessage {
    pub fn new(
        entity: types:: AiEntity,
        request_key: Option<RequestKey>,
        smart_objects: Option<types:: SmartObjects>,
        pawn: Option<types:: AiEntity>,
    ) -> Self {
        Self {
            entity,
            request_key,
            smart_objects,
            pawn,
        }
    }
}

/// A small auxiliary Resource tracking the global AiDecisionRequestedMessage 'cursor' 
/// to enable Change Detection-based conditional triggering for downstream systems.
#[derive(Debug, Default, Resource)]
pub struct LastAiDecisionRequested {
    pub id: Option<MessageId<AiDecisionRequestedMessage>>
}

/// AiDecisionRequestedMessages primarily serve the decision_engine System. 
/// 
/// Unfortunately, that System requires a World access.
/// 
/// We cannot properly access a MessageReader in a System that takes World 
/// (as the readers require mutability, and so reading mutates the World... but 
/// we are holding a World ref that promises to NOT be mutated while we hold it!)
/// 
/// So, instead we use this Resource - we drain Messages into it and empty the Vec 
/// during the maintenance phase of the Cranium schedule.
#[derive(Default, Resource)]
pub struct AiDecisionRequestsBuffer {
    pub buffer: Vec<AiDecisionRequestedMessage>,
}

pub fn drain_decision_requests_into_buffer(
    mut reader: MessageReader<AiDecisionRequestedMessage>,
    mut buffer: ResMut<AiDecisionRequestsBuffer>,
    mut last_seen: ResMut<LastAiDecisionRequested>,
) {
    let mut seen = EntityHashMap::<EntityHashSet>::new();

    for (msg, msg_id) in reader.read_with_id() {
        // Deduplicate requests (within the same tick) by target
        let maybe_pawns = seen.get_mut(&msg.entity);

        // We use EntityHash[Map/Set] for these, which are optimized for, well, Entities, 
        // and not so much for Option<Entity> - so we'll use PLACEHOLDERs as nulls for efficiency.
        let safe_pawn = msg.pawn.unwrap_or(Entity::PLACEHOLDER);

        if let Some(pawns) = maybe_pawns {
            match pawns.contains(&safe_pawn) {
                true => {
                    #[cfg(feature = "logging")]
                    bevy::log::debug!("drain_decision_requests_into_buffer: Skipping duplicate decision request for Entity {:?}", msg.entity);
                    continue
                },
                false => {
                    // We have seen this Entity, but not this (Entity, Pawn) combo.
                    // Just update the Pawns seen for this Entity.
                    pawns.insert(safe_pawn);
                }
            }
        } else {
            // Completely novel Entity, mint a new key AND new pawn set
            seen.insert(msg.entity, EntityHashSet::from([safe_pawn]));
        }
        
        #[cfg(feature = "logging")]
        bevy::log::debug!("drain_decision_requests_into_buffer: Drained message {msg_id:?} into buffer...");
        buffer.buffer.push(msg.clone());
        last_seen.id = Some(msg_id);
    }
}

pub fn cleanup_decision_requests_buffer(
    mut buffer: ResMut<AiDecisionRequestsBuffer>,
) {
    let bufsize = buffer.buffer.len();
    if bufsize > 0 {
        #[cfg(feature = "logging")]
        bevy::log::debug!("cleanup_decision_requests_buffer: Dropping the stale decision requests buffer ({} messages).", buffer.buffer.len());
        buffer.buffer.clear();
    }
}

/// Compatibility bridge for callers still raising the EntityEvent form.
///
/// NOTE: this reintroduces a flush hop — `Commands::trigger` is deferred, so the
/// request reaches the queue only after the next sync point. Prefer the Message
/// form in new code, and write it with a `MessageWriter`, not `Commands`.
pub fn bridge_decision_request_event(
    on: On<AiDecisionRequested>,
    mut writer: MessageWriter<AiDecisionRequestedMessage>,
) {
    let e = on.event();
    writer.write(AiDecisionRequestedMessage {
        entity: e.entity,
        request_key: e.request_key.clone(),
        smart_objects: e.smart_objects.clone(),
        pawn: on.pawn.clone(),
    });
}


/// An Event that signals that Cranium is handing off to the user code by running 
/// any registered ActionHandlers.
/// 
/// Primarily used as a trigger to kick off a System that handles calling an appropriate 
/// user function from the registry - the registry is NonSend, so we are doing this in 
/// a separate System to not force the main decision logic to run on the main thread.
#[derive(Message, Debug)]
pub struct AiActionDispatchToUserCode {
    /// The AI that picked this Action for execution. 
    pub entity: Entity,

    /// Identifier for the handling event (e.g. "GoTo"). 
    /// This is effectively a link to the *implementation* of the action. 
    pub action_key: crate::types::ActionKey,

    /// Human-readable primary identifier; one action_key may handle distinct action_names 
    /// (e.g. action_key "GoTo" may cover action_names "Walk", "Run", "Flee", etc.).
    /// In other words, this is what this action represents *semantically*, and is less likely
    /// to change for technical purposes.
    pub action_name: String,

    /// The Context of the Action, i.e. the static input(s) we scored against.
    pub action_context: crate::types::ActionContextRef,

    /// The Utility score; this is so that we can decide whether to possibly 
    /// override this with a higher-priority Action later on.
    pub action_score: crate::types::ActionScore,
}

impl AiActionDispatchToUserCode {
    pub fn new(
        ai_owner: Entity,
        action_key: crate::types::ActionKey,
        action_name: String,
        action_context: ActionContext,
        action_score: crate::types::ActionScore,
    ) -> Self {
        
        #[cfg(feature = "logging")]
        bevy::log::debug!(
            "Creating a new AiActionDispatchToUserCode event for {:?} with key {:?} ({:?})",
            ai_owner,
            action_key,
            action_name
        );

        let wrapped_ctx = action_context;

        Self {
            entity: ai_owner,
            action_key: action_key,
            action_name: action_name,
            action_context: wrapped_ctx,
            action_score: action_score,
        }
    }
}


#[derive(Message)]
pub struct NoDecisionMessage {
    pub entity: Option<Entity>,
    pub request_key: Option<RequestKey>,
    /// IMPORTANT: If used, comments are meant to be hardcoded, NUL-TERMINATED strings. 
    /// The nul at the end is critical for FFI-safety - failure to include may cause panics. 
    pub comment: Option<&'static str>,
}


#[cfg(test)]
mod tests {
    #[cfg(feature = "logging")]
    use bevy::log::LogPlugin;
    use bevy::{app::ScheduleRunnerPlugin};
    use super::*;
    use crate::ai::AIController;

    #[derive(Component)]
    struct TestContextData {
        _foo: u8,
        _bar: i8,
    }

    #[derive(Debug, Default, Event)]
    struct TestActionEvent;

    fn setup_test_entity(
        mut commands: Commands,
    ) {
        let entity_cmds = commands.spawn(
            AIController::default()
        );

        let entity = entity_cmds.id();

        let ctx2 = commands.spawn(
            TestContextData {
                _foo: 1,
                _bar: 2,
            }
        ).id();

        commands.trigger(AiActionPickedEvent {
            entity: entity.into(),
            payload: AiActionPickedPayload { 
                action_name: "TestAction".into(),
                action_key: "TestActionEvent".into(),
                action_context: ctx2,
                action_score: 1.,
                request_key: None,
            }
        });
    }

    fn dispatch_events(
        trigger: On<AiActionPickedEvent>,
        mut commands: Commands,
    ) {
        let evt = trigger.event();
        let actionkey = evt.payload.action_key.as_str();

        match actionkey {
            "TestActionEvent" => { commands.trigger(TestActionEvent) }
            _ => { panic!("Unrecognized Action Key: {}", actionkey) }
        };
    }

    fn handle_event(
        trigger: On<TestActionEvent>
    ) {
        let _evt = trigger.event();
        #[cfg(feature = "logging")]
        bevy::log::debug!("Processing event {:?}", _evt);
    }

    #[test]
    fn test_run_action() {
        let mut app = App::new();

        app
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_once()),
            #[cfg(feature = "logging")]
            LogPlugin { 
                level: bevy::log::Level::DEBUG, 
                custom_layer: |_| None, 
                filter: "wgpu=error,bevy_render=info,bevy_ecs=info".to_string(),
                fmt_layer: |_| None,
            }
        ))
        .add_systems(Startup, setup_test_entity)
        .add_observer(dispatch_events)
        .add_observer(handle_event)
        ;

        app.run();
    }
}
