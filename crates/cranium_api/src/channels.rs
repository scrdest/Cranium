use core::time::Duration;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::{platform::sync::{OnceLock}};
use cranium_core::events::{AiActionPicked, AiDecisionRequested};
use crossbeam_channel;
use cranium_ffi::{ApiInMsg, ApiOutMsg, EntityOperation, HostIdType, HostMapped, NativeHostIdType};

use crate::spawn;

const DEFAULT_IN_CHANNEL_BOUND: usize = 10_000;
const DEFAULT_OUT_CHANNEL_BOUND: usize = 10_000;
const MAX_FULL_TICKS_FOR_MAINTENANCE: usize = 10;


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
pub(crate) static OUT_CHANNEL: OnceLock<crossbeam_channel::Receiver<ApiOutMsg>> = OnceLock::new();


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
    pub sender: crossbeam_channel::Sender<ApiOutMsg>
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
    pub(crate) receiver: crossbeam_channel::Receiver<ApiOutMsg>
}

#[derive(Message)]
pub(crate) struct DecisionRequestedMsg<I: HostIdType> {
    /// An identifier of the request so we can tie the response back to it neatly.
    request_key: String,

    /// The AI-driven entity we are requesting an update from.
    target: I,
}


/// This is effectively a local buffer of ApiOutMsgs we are about to emit.
/// Similar idea as Commands, but does not touch the World - just the ApiOutputChannel Resource.
#[derive(Message, Debug)]
pub(crate) struct QueuedApiOutMessage(pub(crate) ApiOutMsg);


/// A System that handles receiving and applying updates from the user application to the Cranium app.
fn process_input_messages(
    in_channel: ResMut<ApiInputChannel>, 
    mut exit_writer: MessageWriter<AppExit>,
    mut message_queue: MessageWriter<QueuedApiOutMessage>, 
    mut decisions_requested_writer: MessageWriter<DecisionRequestedMsg<NativeHostIdType>>,
    mut removal_rq_writer: MessageWriter<HostEntityRequestRemovalMessage<NativeHostIdType>>,
    mut upsert_rq_writer: MessageWriter<spawn::HostSpawnRequestMsg<NativeHostIdType>>,
) {
    in_channel.receiver.try_iter().enumerate().for_each(|(i, msg)| {
        bevy::log::debug!("Received input message {:?} - {:?}", i, msg);
        match msg {
            ApiInMsg::Ping => {
                message_queue.write(QueuedApiOutMessage(ApiOutMsg::Pong));
                bevy::log::debug!("Queued up a Pong response");
            },

            ApiInMsg::Shutdown => {
                bevy::log::debug!("Cranium exiting on host's request..."); 
                exit_writer.write(AppExit::Success);
            },

            ApiInMsg::SyncBatch { ops } => {
                bevy::log::debug!("Processing an inbound batch of operations..."); 
                ops.iter().for_each(
                    |raw_op| {
                        match raw_op {
                            EntityOperation::RemoveEntity { host_id } => {
                                removal_rq_writer.write(HostEntityRequestRemovalMessage { target_host_id: host_id.clone() });
                            },
                            EntityOperation::UpsertEntity { host_id, components } => {
                                upsert_rq_writer.write(spawn::HostSpawnRequestMsg { 
                                    host_id: host_id.clone(), 
                                    payload: components.clone() 
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


fn process_queued_output_messages<const TIMEOUT_SECONDS: u64>(
    out_channel: ResMut<ApiOutputChannel>,
    mut message_queue: MessageReader<QueuedApiOutMessage>, 
) {
    for (queued_msg, msg_id) in message_queue.read_with_id() {
        let result = out_channel.sender.send_timeout(queued_msg.0.clone(), Duration::from_secs(TIMEOUT_SECONDS));
        match result {
            Ok(_) => {
                bevy::log::debug!("Sent a message (ID: {}) to the API output channel - {:?}", msg_id, queued_msg);
            }
            Err(err) => {
                bevy::log::error!("Failed to send a message (ID: {}) to the API output channel - Error: {}", msg_id, err);
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
    target_host_id: T,
}

/// This represents that we have decided that a HostMapped Cranium Entity needs to go. 
/// 
/// This is a separate Message to enable Other Stuff to monitor for this specific situation, 
/// generally for the purpose of detaching from the targetted Entity cleanly.
/// 
/// The core cleanup is guaranteed to only run at the end of the update schedule.
#[derive(Message)]
pub(crate) struct HostEntityRemovalTriggered {
    entity: Entity
}

#[derive(Default, Resource)]
pub struct HostIdToEntityRegistry<T: HostIdType>(pub HashMap<T, Entity>);

#[derive(Default, Resource)]
pub struct EntityToHostIdRegistry<T: HostIdType>(HashMap<Entity, T>);


fn host_entity_removal_request_processor<T: HostIdType + 'static> (
    mapping_registry: Res<HostIdToEntityRegistry<T>>, 
    mut msg_reader: MessageReader<HostEntityRequestRemovalMessage<T>>,
    mut msg_writer: MessageWriter<HostEntityRemovalTriggered>,
) {
    let removals = msg_reader.read_with_id().filter_map(
        |(msg, _msg_id)| {
            mapping_registry.0.get(&msg.target_host_id)
        }
    ).map(|entity| HostEntityRemovalTriggered { entity: *entity });

    msg_writer.write_batch(removals);
}


/// The System counterpart of HostEntityRemovalTriggered Messages. 
/// Handles the core job of that Message - despawning HostMapped entities
fn host_entity_removal_executor<T: HostIdType + 'static> (
    mut msg_reader: MessageReader<HostEntityRemovalTriggered>,
    mut commands: Commands, 
    mut from_hostmapping: ResMut<HostIdToEntityRegistry<T>>,
    mut to_hostmapping: ResMut<EntityToHostIdRegistry<T>>,
) {
    msg_reader.read_with_id().for_each(|(msg, _msg_id)| {
        // Despawn the Entity proper.
        commands
        .get_entity(msg.entity)
        .ok()
        .map(|mut ecmd| { 
            ecmd.despawn()
        });

        // Remove the deleted Host ID from the Host2Cranium map.
        to_hostmapping.0
            .get(&msg.entity)
            .map(|hostid| {
                from_hostmapping.0.remove(hostid);
            });


        // Remove the deleted Host ID from the Cranium2Host map.
        to_hostmapping.0.remove(&msg.entity);
    });
}

/// A System that simply converts 
fn decision_requested_msg_handler<I: HostIdType + 'static>(
    host_id_registry: Res<HostIdToEntityRegistry<I>>,
    mut in_messages: MessageReader<DecisionRequestedMsg<I>>,
    mut commands: Commands,
) {
    in_messages.read_with_id().for_each(|(msg, msg_id)| {
        host_id_registry.0.get(&msg.target)
        .map_or_else(
            || {
                bevy::log::error!("Decision requested for an unrecognized/untracked Entity! MsgId: {:?} | RqKey: {:?} | HostId: {:?}", 
                    msg_id, 
                    msg.request_key, 
                    msg.target
                );
            }, 
            |local_entity| {
                commands.trigger(AiDecisionRequested {
                    entity: *local_entity,
                    request_key: Some(msg.request_key.clone()),
                    smart_objects: None // TODO: review!
                });
            }
        )
        ;
    });
}

fn decision_output_handler<I: HostIdType + 'static + Into<NativeHostIdType>> (
    trigger: On<AiActionPicked>,
    query: Query<&HostMapped<I>>,
    mut message_queue: MessageWriter<QueuedApiOutMessage>,
) {
    let host_mapped_agent_id = query.get(trigger.event_target()).and_then(|comp| {
        Ok(comp.host_id.clone())
    }).unwrap();

    let host_mapped_context = query.get(trigger.action_context).and_then(|comp| {
        Ok(comp.host_id.clone())
    }).unwrap();

    message_queue.write(QueuedApiOutMessage(ApiOutMsg::ActionChosen { 
        host_agent_id: host_mapped_agent_id.into(), 
        host_action_id: trigger.action_key.clone(), 
        host_context_id: host_mapped_context.into(), 
    }))
    ;
}


/// A maintenance system that tries to save clogged output channels by popping oldest messages off of it. 
fn check_output_channel_for_clogs(
    out_channel: ResMut<ApiOutputChannelMaintenance>,
    mut channel_full_ticks: Local<usize>,
) {
    let is_full = out_channel.receiver.capacity().unwrap_or_default() > 0 && out_channel.receiver.is_full();
    if is_full {
        *channel_full_ticks = channel_full_ticks.saturating_add(1);
    }

    if *channel_full_ticks > MAX_FULL_TICKS_FOR_MAINTENANCE {
        // Pop a message from the channel to hopefully unclog it.
        bevy::log::debug!("Cranium output channel clogged! Attempting a receive...");
        let _ = out_channel.receiver.recv();
    }
}

/// A Plugin that adds Channels for communication between Bevy worlds and external code.
#[derive(Default)]
pub struct ApiChannelsPlugin {
    in_channel_bound: Option<usize>,
    out_channel_bound: Option<usize>,
}

impl ApiChannelsPlugin {
    pub fn with_bounds(in_bound: Option<usize>, out_bound: Option<usize>) -> Self {
        Self {
            in_channel_bound: in_bound,
            out_channel_bound: out_bound,
        }
    }

    pub fn with_bounds_tuple(bounds: Option<(usize, usize)>) -> Self {
        Self {
            in_channel_bound: bounds.map(|b| b.0),
            out_channel_bound: bounds.map(|b| b.1),
        }
    }
}


impl Plugin for ApiChannelsPlugin {
    fn build(&self, app: &mut App) {
        // Wire up the input channel...
        let in_bound = self.in_channel_bound.unwrap_or(DEFAULT_IN_CHANNEL_BOUND);
        let (in_snd, in_rcv) = crossbeam_channel::bounded::<ApiInMsg>(in_bound);
        let channel_result = IN_CHANNEL.set(in_snd.clone());

        channel_result.expect("ApiChannelsPlugin IN_CHANNEL is already initialized (somehow)!");
        
        // Resources for handling input channel stuff.
        app.insert_resource(ApiInputChannel { receiver: in_rcv });
        app.insert_resource(ApiInputChannelMock { sender: in_snd });

        // ...and the output channel.
        let out_bound = self.out_channel_bound.unwrap_or(DEFAULT_OUT_CHANNEL_BOUND);
        let (out_snd, out_rcv) = crossbeam_channel::bounded::<ApiOutMsg>(out_bound);
        let channel_result = OUT_CHANNEL.set(out_rcv.clone());
        
        channel_result.expect("ApiChannelsPlugin OUT_CHANNEL is already initialized (somehow)!");

        let start_sent = out_snd.send(ApiOutMsg::CraniumStarted);
        start_sent.expect("Failed to send initial message on the OUT channel.");
        
        // Resources for handling output channel stuff.
        app.insert_resource(ApiOutputChannel { sender: out_snd });
        app.insert_resource(ApiOutputChannelMaintenance { receiver: out_rcv });

        // Resources for mapping Host IDs to Cranium IDs and back.
        let from_host_id_reg: HashMap<NativeHostIdType, Entity> = HashMap::new();
        let to_host_id_reg: HashMap<Entity, NativeHostIdType> = HashMap::new();
        app.insert_resource(HostIdToEntityRegistry(from_host_id_reg));
        app.insert_resource(EntityToHostIdRegistry(to_host_id_reg));

        // Wire up the systems processing the channels.
        app.add_systems(PreUpdate, 
            (
                process_input_messages, 
            )
        );

        // Output handling
        app.add_message::<QueuedApiOutMessage>();

        app.add_systems(Last, (
                check_output_channel_for_clogs,
                process_queued_output_messages::<1>,
            ).chain()
        );


        // Entity removal core pipeline.
        // 
        // Note that the plugin only covers the NativeHostIdType impl - if users want to use their own custom 
        // Host ID types, they will need to write their own plugin/app setup to add analogous Systems for that.
        // 
        // Also note that those are ultimately processors on a Message bus - by design, you can hook up 
        // additional consumer Systems to these buses to add additional handling for Host entity removals. 
        app.add_message::<HostEntityRequestRemovalMessage<NativeHostIdType>>();
        app.add_systems(PreUpdate, host_entity_removal_request_processor::<NativeHostIdType>);
        
        app.add_message::<HostEntityRemovalTriggered>();
        app.add_systems(Last, host_entity_removal_executor::<NativeHostIdType>);


        // Entity upsert core pipeline.
        // 
        // Note that the plugin only covers the NativeHostIdType impl - if users want to use their own custom 
        // Host ID types, they will need to write their own plugin/app setup to add analogous Systems for that. 
        // 
        // Also note that those are ultimately processors on a Message bus - by design, you can hook up 
        // additional consumer Systems to these buses to add additional handling for Host entity upserts. 
        app.add_message::<spawn::HostSpawnRequestMsg<NativeHostIdType>>();
        app.add_message::<spawn::HostSpawnResponseSuccessMsg<NativeHostIdType>>();
        app.add_message::<spawn::HostSpawnResponseErrorMsg<NativeHostIdType>>();
        app.add_systems(
            PreUpdate, 
            (
                spawn::process_remote_spawn_entity_request::<NativeHostIdType>,
                (
                    spawn::forward_spawn_error_signals,
                    spawn::forward_spawn_success_signals,
                )
            ).chain()
        );

        // Async decision request/response handling.
        app.add_message::<DecisionRequestedMsg::<NativeHostIdType>>();
        app.add_systems(PreUpdate, decision_requested_msg_handler::<NativeHostIdType>);
        app.add_observer(decision_output_handler::<NativeHostIdType>);

    }
}
