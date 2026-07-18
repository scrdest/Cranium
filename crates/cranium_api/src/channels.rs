/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
use core::time::Duration;

use cranium_core::bevy::platform::collections::HashMap;
use cranium_core::bevy::platform::sync::Arc;
use cranium_core::bevy::prelude::*;
use cranium_core::bevy::{platform::sync::{OnceLock}};
use cranium_core::bevy::log;
use cranium_core::ai::AIController;
use cranium_core::actions::ActionKey;
use cranium_core::events::{AiActionPicked, AiDecisionRequested, NoDecisionMessage};
use cranium_core::smart_object::SmartObjects;
use crossbeam_channel;
use cranium_ffi::{ApiInMsg, EntityOperation, HostIdType, HostMapped, NativeHostIdType, RequestKey, StagedApiOutMsg};

use crate::spawn;

pub(crate) const DEFAULT_IN_CHANNEL_BOUND: usize = 100;
pub(crate) const DEFAULT_OUT_CHANNEL_BOUND: usize = 100;
pub(crate) const MAX_FULL_TICKS_FOR_MAINTENANCE: usize = 10;


/// The Channel that routes messages FROM the host TO the Cranium Bevy App. 
/// 
/// If Cranium is our 'brain', this would be our 'afferent nerves'.
/// 
/// This is used e.g. to sync state updates from the user to the AI engine.
pub(crate) static IN_CHANNEL: OnceLock<crossbeam_channel::Sender<ApiInMsg>> = OnceLock::new();


/// The Channel that routes messages FROM Cranium back TO the user. 
/// 
/// If Cranium is our 'brain', this would be our 'efferent nerves'.
/// 
/// This is used to exfiltrate data such as chosen Actions out of the AI engine and into the host.
pub(crate) static OUT_CHANNEL: OnceLock<crossbeam_channel::Receiver<StagedApiOutMsg>> = OnceLock::new();


/// A Resource that represents Read access to the Input channel, 
/// i.e. accessing messages sent BY the host application TO Cranium. 
#[derive(Resource)]
pub struct ApiInputChannel {
    pub receiver: crossbeam_channel::Receiver<ApiInMsg>
}


/// A Resource that represents Write access to the Output channel, 
/// i.e. messages sent BY Cranium TO the host application.
#[derive(Resource)]
pub struct ApiOutputChannel {
    pub sender: crossbeam_channel::Sender<StagedApiOutMsg>
}


/* --  WARNING:  --
Both Resources below are not meant for public consumption; 
they are specialized tools for niche purposes only!
*/

/// A Resource that represents Write access to the Input channel. 
/// 
/// Intended for mocking purposes - NOT for production use!
#[derive(Resource)]
pub(crate) struct ApiInputChannelMock {
    pub(crate) sender: crossbeam_channel::Sender<ApiInMsg>
}


/// A Resource that represents Read access to the Output channel. 
/// 
/// Intended for 'plunger' Systems that deal with clogged Output pipes.
#[derive(Resource)]
pub(crate) struct ApiOutputChannelMaintenance {
    pub(crate) receiver: crossbeam_channel::Receiver<StagedApiOutMsg>,
    pub(crate) pop_messages: bool, 
}

#[derive(Message)]
pub(crate) struct DecisionRequestedMsg<I: HostIdType> {
    /// An identifier of the request so we can tie the response back to it neatly.
    request_key: RequestKey,

    /// The AI-driven entity we are requesting an update from.
    target: I,
}


/// This is effectively a local buffer of ApiOutMsgs we are about to emit.
/// Similar idea as Commands, but does not touch the World - just the ApiOutputChannel Resource.
#[derive(Message, Debug)]
pub(crate) struct QueuedApiOutMessage(pub(crate) StagedApiOutMsg);


/// A System that handles receiving and applying updates from the user application to the Cranium app.
pub(crate) fn process_input_messages(
    in_channel: ResMut<ApiInputChannel>, 
    mut exit_writer: MessageWriter<AppExit>,
    mut message_queue: MessageWriter<QueuedApiOutMessage>, 
    mut decisions_requested_writer: MessageWriter<DecisionRequestedMsg<NativeHostIdType>>,
    mut removal_rq_writer: MessageWriter<HostEntityRequestRemovalMessage<NativeHostIdType>>,
    mut upsert_rq_writer: MessageWriter<spawn::HostSpawnRequestMsg<NativeHostIdType>>,
) {
    in_channel.receiver.try_iter().enumerate().for_each(|(i, msg)| {
        #[cfg(feature = "logging")]
        log::debug!("Received input message {:?} - {:?}", i, msg);
        match msg {
            ApiInMsg::Ping => {
                message_queue.write(QueuedApiOutMessage(StagedApiOutMsg::Pong));
                #[cfg(feature = "logging")]
                log::debug!("Queued up a Pong response");
            },

            ApiInMsg::Shutdown => {
                #[cfg(feature = "logging")]
                log::debug!("Cranium exiting on host's request..."); 
                exit_writer.write(AppExit::Success);
            },

            ApiInMsg::SyncBatch { ops } => {
                #[cfg(feature = "logging")]
                log::debug!("Processing an inbound batch of operations..."); 
                ops.iter().for_each(
                    |raw_op| {
                        match raw_op {
                            EntityOperation::RemoveEntity { 
                                host_id ,
                                request_key
                            } => {
                                removal_rq_writer.write(
                                    HostEntityRequestRemovalMessage { 
                                        target_host_id: host_id.clone(),
                                        request_key: request_key.clone(),
                                    });
                            },
                            EntityOperation::UpsertEntity { 
                                host_id, 
                                components , 
                                request_key, 
                            } => {
                                upsert_rq_writer.write(spawn::HostSpawnRequestMsg { 
                                    host_id: host_id.clone(), 
                                    payload: components.clone(), 
                                    request_key: *request_key,
                                });
                            }
                        }
                    }
                );
            }

            ApiInMsg::RequestDecision { targets } => {
                decisions_requested_writer.write_batch(
                    // Same thing - we'll let the downstream readers figure out how to handle these.
                    targets.iter().map(|t| DecisionRequestedMsg { 
                        request_key: t.0.clone(), 
                        target: t.1.clone(), 
                    })
                );
            },
        };
    });
}


pub(crate) fn process_queued_output_messages<const TIMEOUT_SECONDS: u64>(
    out_channel: ResMut<ApiOutputChannel>,
    mut message_queue: MessageReader<QueuedApiOutMessage>, 
) {
    for (queued_msg, msg_id) in message_queue.read_with_id() {
        let output_message = queued_msg.0.clone().into();

        let result = out_channel.sender.send_timeout(
            output_message, 
            Duration::from_secs(TIMEOUT_SECONDS)
        );

        match result {
            Ok(_) => {
                #[cfg(feature = "logging")]
                log::debug!("Sent a message (ID: {}) to the API output channel - {:?}", msg_id, queued_msg);
            }
            Err(err) => {
                #[cfg(feature = "logging")]
                log::error!("Failed to send a message (ID: {}) to the API output channel - Error: {}", msg_id, err);
                // Stop processing messages on the first failure; we'll get 'em next time!
                break;
            }
        }
    }
}


/// This represents queueing up a HostMapped entity for (1) resolution & (2) removal. 
/// 
/// Neither of these two is guaranteed to happen at this stage yet - the host may request 
/// Cranium to delete a host object Cranium does not track (for some reason), or which cannot 
/// be safely removed due to something else depending on it (unlikely, but possible).
/// 
/// TL;DR: This is just a polite 'stop tracking this please' request. 
/// The ACTUAL deletion is handled by an Observer (HostEntityRemovalTriggered) later (if we are lucky).
#[derive(Message)]
pub(crate) struct HostEntityRequestRemovalMessage<T: HostIdType> {
    pub(crate) target_host_id: T,
    pub request_key: RequestKey,
}

/// This represents that we have decided that a HostMapped Cranium Entity needs to go. 
/// 
/// This is a separate Message to enable Other Stuff to monitor for this specific situation, 
/// generally for the purpose of detaching from the targetted Entity cleanly.
/// 
/// The core cleanup is guaranteed to only run at the end of the update schedule.
#[derive(Message)]
pub(crate) struct HostEntityRemovalTriggered {
    pub(crate) entity: Entity,
    pub request_key: RequestKey,
}

#[derive(Default, Resource)]
pub struct HostIdToEntityRegistry<T: HostIdType>(pub HashMap<T, Entity>);

#[derive(Default, Resource)]
pub struct EntityToHostIdRegistry<T: HostIdType>(pub(crate) HashMap<Entity, T>);


/// A System that simply converts DecisionRequestedMsgs into Observer triggers 
/// for processing Decisions for each AI (which ultimately get emitted back as
/// AiActionPicked Events and turned into ApiOutMsg::ActionChosen Messages).
/// 
/// Note that a request is NOT guaranteed to trigger an AI decision! 
/// 
/// Cranium reserves the right to suppress requests that do not target 
/// an actual AI-enabled Entity or have no possible Actions to evaluate; 
/// then of course even if the Decision Engine runs, we may still wind up 
/// not finding any valid Actions for the current state of the AI agent.
pub(crate) fn decision_requested_msg_handler<I: HostIdType + 'static>(
    host_id_registry: Res<HostIdToEntityRegistry<I>>,
    so_query: Query<&SmartObjects, With<AIController>>, 
    mut in_messages: MessageReader<DecisionRequestedMsg<I>>,
    mut failure_messages: MessageWriter<NoDecisionMessage>,
    mut commands: Commands,
) {
    in_messages.read_with_id().for_each(|(msg, msg_id)| {
        host_id_registry
        .0.get(&msg.target)
        .map_or_else(
            || {
                #[cfg(feature = "logging")]
                log::error!("Decision requested for an unrecognized/untracked Entity! MsgId: {:?} | RqKey: {:?} | HostId: {:?}", 
                    msg_id, 
                    msg.request_key, 
                    msg.target
                );
            }, 
            |local_entity| {
                let smart_objects = so_query
                    .get(*local_entity)
                    .map(|so_data| so_data)
                    .ok()
                ;
                
                match smart_objects {
                    Some(sos) => {
                        #[cfg(feature = "logging")]
                        log::debug!(
                            "Triggered a Decision request for Entity {} with {} SmartObject ActionSets.", 
                            local_entity,
                            sos.actionset_refs.len()
                        );
                        commands.trigger(AiDecisionRequested {
                            entity: *local_entity,
                            request_key: Some(msg.request_key.clone()),
                            smart_objects: Some(sos.clone()),
                        });
                    },

                    None => {
                        #[cfg(feature = "logging")]
                        log::debug!(
                            "Ignored a Decision request for Entity {} - no SmartObjects available.", 
                            local_entity,
                        );
                        failure_messages.write(NoDecisionMessage {
                            entity: *local_entity,
                            request_key: Some(msg.request_key.clone()),
                            comment: Some("No SmartObjects available.")
                        });
                    }
                }
            }
        )
        ;
    });
}

#[derive(Default, Resource)]
pub struct HostActionIdMap<I: HostIdType + 'static> {
    pub(crate) key_to_host_id_map: HashMap<Arc<ActionKey>, Arc<I>>,
    pub(crate) host_id_to_key_map: HashMap<Arc<I>, Arc<ActionKey>>,
}

impl<I: HostIdType + 'static> HostActionIdMap<I> {
    pub fn get_host_id(&self, key: &ActionKey) -> Option<&Arc<I>> {
        self.key_to_host_id_map.get(key)
    }

    pub fn get_action_key(&self, key: &I) -> Option<&Arc<ActionKey>> {
        self.host_id_to_key_map.get(key)
    }

    /// Registers a (bijective) mapping from HostId (H) to an ActionKey (A). 
    /// 
    /// Returns self for a fluent API-style interface.
    /// 
    /// Critically, the bijective-ness means the mapping is strictly 1:1! 
    /// Any insert(H, A) inserts a unique H->A mapping and a unique A->H mapping.
    /// An ActionKey can therefore be mapped to only one HostId and vice versa. 
    /// 
    /// If either H or A are already mapped to something else, the old relationship will
    /// be replaced and a warning log entry will be emitted for each affected 'direction'.
    /// 
    /// Generally speaking, this would ideally not happen at all, and if it does, 
    /// we'd expect to see warnings emitted for both H->A and A->H directions. 
    pub fn insert(&mut self, host_id: I, action_key: ActionKey) -> &mut Self {
        let arc_action_key = Arc::new(action_key);
        let arc_host_id = Arc::new(host_id);

        self.key_to_host_id_map
            .insert(arc_action_key.clone(), arc_host_id.clone())
            .map(|old| {
                #[cfg(feature = "logging")]
                log::warn!(
                    "HostActionIdMap insert of ActionKey '{:?}'->'{:?}' is overwriting a previous HostId mapping '{:?}'",
                    arc_action_key.as_ref(),
                    arc_host_id.as_ref(),
                    old,
                )
            }
        );

        self.host_id_to_key_map
            .insert(arc_host_id.clone(), arc_action_key.clone())
            .map(|old| {
                #[cfg(feature = "logging")]
                log::warn!(
                    "HostActionIdMap insert of HostId '{:?}'->'{:?}' is overwriting a previous ActionKey mapping '{:?}'",
                    arc_host_id.as_ref(),
                    arc_action_key.as_ref(), 
                    old,
                )
            }
        );
        self
    }

    /// Unregisters a (bijective) mapping from HostId to an ActionKey.
    pub fn remove(&mut self, host_id: &I, action_key: &ActionKey) -> (Option<Arc<ActionKey>>, Option<Arc<I>>) {
        let keypop = self.host_id_to_key_map.remove(host_id);
        let hostpop = self.key_to_host_id_map.remove(action_key);
        (keypop, hostpop)
    }
}

pub fn decision_output_handler<I: HostIdType + 'static + Into<NativeHostIdType>> (
    trigger: On<AiActionPicked>,
    query: Query<&HostMapped<I>>,
    host_action_id_registry: Res<HostActionIdMap<I>>,
    mut message_queue: MessageWriter<QueuedApiOutMessage>,
) {
    let tgt = trigger.event_target();
    let host_mapped_agent_id = query.get(tgt).and_then(|comp| {
        Ok(comp.host_id.clone())
    }).map_err(|err| {
        #[cfg(feature = "logging")]
        log::error!(
            "decision_output_handler - failed to locate a HostMapped component on the AiActionPicked Target {:?}.
            Error: '{}'.  
            ActionChosen message will not be send due to invariant violation. ",
            tgt,
            err, 
        )
    });

    let host_mapped_context = query.get(trigger.action_context).and_then(|comp| {
        Ok(comp.host_id.clone())
    }).map_err(|err| {
        #[cfg(feature = "logging")]
        log::error!(
            "decision_output_handler - failed to locate a HostMapped component on the AiActionPicked Context {:?}. 
            Error: '{}'. 
            ActionChosen message will not be send due to invariant violation. ",
            trigger.action_context,
            err, 
        )
    });

    let host_mapped_action = host_action_id_registry.get_host_id(&trigger.action_key).or_else(|| {
        #[cfg(feature = "logging")]
        log::error!(
            "decision_output_handler - failed to match a HostId for an AiActionPicked Action {:?}. 
            ActionChosen message will not be send due to invariant violation. ",
            &trigger.action_key,
        );
        None
    });

    match (host_mapped_agent_id, host_mapped_context, host_mapped_action) {
        (Ok(agent_id), Ok(context), Some(action)) => {
            message_queue.write(
                QueuedApiOutMessage(
                    StagedApiOutMsg::ActionChosen { 
                        host_agent_id: agent_id.into(), 
                        host_action_id: action.as_ref().to_owned().into(), 
                        host_context_id: context.into(), 
                        request_key: trigger.request_key.unwrap_or_default(),
                    }
                )
            );
        }
        _ => ()
    }
    ;
}


pub fn decision_failed_handler<I: HostIdType + 'static + Into<NativeHostIdType>> (
    query: Query<&HostMapped<I>>,
    mut input_msgs: MessageReader<NoDecisionMessage>,
    mut message_queue: MessageWriter<QueuedApiOutMessage>,
) {
    let messages = input_msgs.read().map(|msg| {
        let host_mapped_agent_id = query
            .get(msg.entity)
            .and_then(|comp| {
                Ok(comp.host_id.clone())
            }
        ).unwrap();

        QueuedApiOutMessage(
            StagedApiOutMsg::NoActionChosen { 
                host_agent_id: host_mapped_agent_id.into(), 
                request_key: msg.request_key.unwrap_or_default(),
                comment: msg.comment.map(|m| Arc::new(m.to_string())),
            },
        )
    })
    ;

    message_queue.write_batch(messages);
}


/// A maintenance system that tries to save clogged output channels by popping oldest messages off of it. 
pub fn check_output_channel_for_clogs(
    out_channel: ResMut<ApiOutputChannelMaintenance>,
    mut channel_full_ticks: Local<usize>,
) {
    let is_full = out_channel.receiver.capacity().unwrap_or_default() > 0 && out_channel.receiver.is_full();
    match is_full {
        true => *channel_full_ticks = channel_full_ticks.saturating_add(1),
        false => *channel_full_ticks = 0,
    }

    if *channel_full_ticks > MAX_FULL_TICKS_FOR_MAINTENANCE {
        match out_channel.pop_messages {
            true => {
                // Pop a message from the channel to hopefully unclog it.
                #[cfg(feature = "logging")]
                log::warn!("Cranium output channel clogged! Attempting a receive...");
                let drop_msg = out_channel.receiver.recv();
                if let Ok(dropped) = drop_msg {
                    #[cfg(feature = "logging")]
                    log::warn!("Output unclog: dropping message {:?}", dropped)
                };
            },
            false => {
                // Just alert that we're full up.
                #[cfg(feature = "logging")]
                log::warn!(
                    "Cranium output channel clogged! Messages will not be produced until some have been received!"
                );
            }
        }
    }
}
