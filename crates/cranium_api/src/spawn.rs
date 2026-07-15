/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
// Needed in-scope for insert_reflect() call
use bevy::ecs::reflect::ReflectCommandExt;

use bevy::platform::sync::Arc;
// We need this import or Cargo complains about panic unwinding vOv
use bevy::prelude::*;

use bevy::reflect::{
    serde::TypedReflectDeserializer,
};

use cranium_ffi::{HostIdType, HostMapped, NativeHostIdType, RequestKey};
use serde_json::Value;
use serde::de::DeserializeSeed;

use crate::channels::{EntityToHostIdRegistry, HostEntityRemovalTriggered, HostEntityRequestRemovalMessage, HostIdToEntityRegistry, QueuedApiOutMessage};


// Shamelessly, uh, 'borrowed', from Bevy's own source code with
// some possible modifications to suit the needs of this project.
// (credit: https://github.com/bevyengine/bevy/blob/0eac08ae5da33f39d64ad148740c34c14b38c481/crates/bevy_remote/src/builtin_methods.rs#L1908)
/// Given a collection of component paths and their associated serialized values (`components`),
/// return the associated collection of deserialized reflected values.
fn deserialize_components(
    type_registry: &bevy::reflect::TypeRegistry,
    components: bevy::platform::collections::HashMap<String, Value>,
) -> Result<Vec<Box<dyn PartialReflect>>, String> {
    let mut reflect_components = vec![];

    for (component_path, component) in components {
        let Some(component_type) = 
            type_registry.get_with_short_type_path(&component_path) 
                .or_else(|| {type_registry.get_with_type_path(&component_path)})
            else {
                return Err(format!("Unknown component type: `{}`", component_path));
            };
        let reflected: Box<dyn PartialReflect> =
            TypedReflectDeserializer::new(component_type, type_registry)
                .deserialize(&component)
                .map_err(|err| format!("{component_path} is invalid: {err}"))?;
        reflect_components.push(reflected);
    }

    Ok(reflect_components)
}

// Likewise, this function also fell off the back of a lorry, officer, crazy story. 
// (credit: https://github.com/bevyengine/bevy/blob/0eac08ae5da33f39d64ad148740c34c14b38c481/crates/bevy_remote/src/builtin_methods.rs#L1947)
/// Given a collection `reflect_components` of reflected component values, insert them into
/// the given entity (`entity_world_mut`).
fn insert_reflected_components(
    mut entity_world_mut: EntityCommands,
    reflect_components: Vec<Box<dyn PartialReflect>>,
) -> Result<(), String> {
    for reflected in reflect_components {
        entity_world_mut.insert_reflect(reflected);
    }

    Ok(())
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
#[serde(transparent)]
pub struct HostSpawnRequestParams(bevy::platform::collections::HashMap<String, Value>);

/// This represents queueing up a HostMapped entity for resolution and update/insert of Components
/// based on the results (i.e. - if we already track this Host ID -> update, else insert). 
#[derive(Debug, Message)]
pub struct HostSpawnRequestMsg<I: HostIdType> {
    pub payload: Arc<String>,
    pub host_id: I,
    pub request_key: RequestKey,
}

/// Signals a successful Spawn request  - consumed and re-emitted to the output channel as feedback.
#[derive(Debug, Message)]
pub struct HostSpawnResponseSuccessMsg<I: HostIdType> {
    pub entity: Entity,
    pub host_id: I,
    pub comments: Option<Arc<String>>,
    pub request_key: RequestKey,
}

/// Signals a bad Spawn request - consumed and re-emitted to the output channel as feedback.
#[derive(Debug, Message)]
pub struct HostSpawnResponseErrorMsg<I: HostIdType> {
    pub error: Arc<String>,
    pub host_id: I,
    pub request_key: RequestKey,
}

// Completing the unholy trinity of copypasta, the actual handler using both of the above.
// (https://github.com/bevyengine/bevy/blob/0eac08ae5da33f39d64ad148740c34c14b38c481/crates/bevy_remote/src/builtin_methods.rs#L1908)
// This is the most customized of the three, barely bearing any similarity to the original. 
// Rather than operating on exclusive World access, we use good ol' Commands for this, 
// and simply process Messages (inbound and outbound).
pub fn process_remote_spawn_entity_request<I: HostIdType + 'static>(
    app_type_registry: Res<AppTypeRegistry>,
    real_timer: Res<Time<Real>>, 
    mut request_stream: MessageReader<HostSpawnRequestMsg<I>>,
    mut success_response_stream: MessageWriter<HostSpawnResponseSuccessMsg<I>>,
    mut error_response_stream: MessageWriter<HostSpawnResponseErrorMsg<I>>,
    mut from_hostmapping: ResMut<HostIdToEntityRegistry<I>>,
    mut to_hostmapping: ResMut<EntityToHostIdRegistry<I>>,
    mut commands: Commands,
) {
    let type_registry = app_type_registry.read();

    let success_messages = request_stream.read_with_id().filter_map(|(request, request_id)| {
        // IN THEORY we could do all of this in One Big Pipeline with a bunch of and_then()s.
        // However, this is broken down into three logical steps for better legibility.
        #[cfg(feature = "logging")]
        bevy::log::debug!(
            "Processing an Entity Spawn Request (RqID: {}, HostId: {:?})...", 
            request_id,
            request.host_id,
        );

        // (1) Deserialize: request payload -> specmap (component key : fields)
        let parsed: Option<HostSpawnRequestParams> = 
            serde_json::from_str(&request.payload)
            .map_err(|err| {
                #[cfg(feature = "logging")]
                bevy::log::error!(
                    "Error parsing Cranium Entity Spawn Request (RqID: {}, HostId: {:?}, Msg: '{}') - {}", 
                    request_id, 
                    request.host_id, 
                    request.payload,
                    err,
                );
                error_response_stream.write(HostSpawnResponseErrorMsg { 
                    error: Arc::new(err.to_string()), 
                    host_id: request.host_id.clone(),
                    request_key: request.request_key.clone(),
                });
            }
        ).ok();

        // (2) Deserialize: specmap -> actual Components (dynamic; uses the app type registry)
        let reflected = parsed
        .and_then(|p| Some(p.0))
        .and_then(|comp| {
            deserialize_components(&type_registry, comp)
            .map_err(|e| {
                #[cfg(feature = "logging")]
                bevy::log::error!("Error deserializing Components from a Spawn API request (RqID: {}, HostId: {:?}): {:?}", request_id, request.host_id, e); 
                error_response_stream.write(HostSpawnResponseErrorMsg { 
                    error: Arc::new(e), 
                    host_id: request.host_id.clone(), 
                    request_key: request.request_key.clone(), 
                });
            })
            .ok()
        });
        
        // (3) Insert the reflected Components to the Entity and notify success (hopefully)
        let msg = reflected.and_then(|reflect_components| {
            let host_id = Some(&request.host_id);
            let mapped_to_entity = host_id.and_then(|id| from_hostmapping.0.get(id));
            let mut is_update = true;

            let maybe_entity = match mapped_to_entity {
                None => {
                    is_update = false;
                    Ok(commands.spawn_empty())
                },
                Some(&e) => commands.get_entity(e).map_err(
                    |err| {
                        #[cfg(feature = "logging")]
                        bevy::log::error!("Invalid Entity {} (HostId: {:?}) retrieved from the HostIdToEntityRegistry - {}", e, request.host_id, err);
                        err
                    }, 
                )
            };

            let mut entity = maybe_entity.unwrap();
            let entity_id = entity.id();

            if host_id.is_some() {
                if !is_update {
                    // Make sure we've got the host2entity mapping in the Registry
                    from_hostmapping.0.insert(request.host_id.clone(), entity_id);
                    to_hostmapping.0.insert(entity_id, host_id.unwrap().clone());
                }

                entity.insert_if_new(
                    HostMapped::from_value_at_time(
                        host_id.unwrap().clone(), 
                        Some(real_timer.elapsed_wrapped())
                    )
                );
            }
            
            insert_reflected_components(entity, reflect_components)
            .map_err(|e| {
                match is_update {
                    true => {
                        #[cfg(feature = "logging")]
                        bevy::log::error!(
                            "Error updating Components from a Spawn API request (RqID: {}, HostId {:?}): {:?}", 
                            request_id, 
                            request.host_id,
                            e
                        )
                    },
                    false => {
                        #[cfg(feature = "logging")]
                        bevy::log::error!(
                            "Error inserting Components from a Spawn API request (RqID: {}, HostId {:?}): {:?}", 
                            request_id, 
                            request.host_id,
                            e
                        )
                    }
                }
                error_response_stream.write(HostSpawnResponseErrorMsg { 
                    error: Arc::new(e), 
                    host_id: request.host_id.clone(),
                    request_key: request.request_key.clone(),
                });
            })
            .ok()
            .and_then(|_| {
                match is_update {
                    false => {
                        #[cfg(feature = "logging")]
                        bevy::log::debug!(
                            "Successfully spawned an Entity ({}, HostId {:?}) with Components based on Spawn request (RqID: {})", 
                            entity_id, 
                            request.host_id,
                            request_id,
                        )
                    },
                    true => {
                        #[cfg(feature = "logging")]
                        bevy::log::debug!(
                            "Successfully updated an Entity ({}, HostId {:?}) with Components based on Spawn request (RqID: {})", 
                            entity_id, 
                            request.host_id,
                            request_id,
                        )
                    }
                }
                
                Some(HostSpawnResponseSuccessMsg {
                    entity: entity_id, 
                    host_id: request.host_id.clone(), 
                    comments: None,
                    request_key: request.request_key.clone(),
                })
            })
        });
        msg
    });

    success_response_stream.write_batch(success_messages);
}


/// Queues up successful Spawn responses as messages to push out to the output channel.
pub(crate) fn forward_spawn_success_signals(
    mut success_messages: MessageReader<HostSpawnResponseSuccessMsg<NativeHostIdType>>,
    mut message_queue: MessageWriter<QueuedApiOutMessage>, 
) {
    let transformed_msgs = success_messages
        .read()
        .map(|msg| {
            let out_msg = cranium_ffi::StagedApiOutMsg::EntitySpawnSuccessful(
                msg.host_id.clone(), 
                msg.request_key.clone(),
            );
            #[cfg(feature = "logging")]
            bevy::log::debug!("Queuing up a EntitySpawnSuccessful Message for output channel send: {:?}", out_msg);
            QueuedApiOutMessage(out_msg)
        }
    );
    message_queue.write_batch(transformed_msgs);
}


/// Queues up error Spawn responses as messages to push out to the output channel.
pub(crate) fn forward_spawn_error_signals(
    mut success_messages: MessageReader<HostSpawnResponseErrorMsg<NativeHostIdType>>,
    mut message_queue: MessageWriter<QueuedApiOutMessage>, 
) {
    let transformed_msgs = success_messages
        .read()
        .map(|msg| {
            let out_msg = cranium_ffi::StagedApiOutMsg::EntitySpawnError(
                msg.host_id.clone(),
                msg.request_key.clone(),
                msg.error.clone(),
            );
            #[cfg(feature = "logging")]
            bevy::log::debug!("Queuing up a EntitySpawnError Message for output channel send: {:?}", out_msg);
            QueuedApiOutMessage(out_msg)
        }
    );
    message_queue.write_batch(transformed_msgs);
}

/// Signals a successful Spawn request  - consumed and re-emitted to the output channel as feedback.
#[derive(Debug, Message)]
pub struct HostDespawnResponseSuccessMsg<I: HostIdType> {
    pub entity: Entity,
    pub host_id: Option<I>,
    pub comments: Option<Arc<String>>,
    pub request_key: RequestKey,
}

/// Signals a bad Spawn request - consumed and re-emitted to the output channel as feedback.
#[derive(Debug, Message)]
pub struct HostDespawnResponseErrorMsg<I: HostIdType> {
    pub host_id: Option<I>,
    pub error: Arc<String>,
    pub request_key: RequestKey,
}

pub(crate) fn host_entity_removal_request_processor<T: HostIdType + 'static> (
    mapping_registry: Res<HostIdToEntityRegistry<T>>, 
    mut msg_reader: MessageReader<HostEntityRequestRemovalMessage<T>>,
    mut msg_writer: MessageWriter<HostEntityRemovalTriggered>,
    mut err_writer: MessageWriter<HostDespawnResponseErrorMsg<T>>,
) {
    let removals = msg_reader.read_with_id().filter_map(
        |(msg, msg_id)| {
            #[cfg(feature = "logging")]
            bevy::log::debug!(
                "Processing HostEntityRequestRemovalMessage (ID: {:?}) - Target: {:?}",
                msg_id,
                msg.target_host_id
            );
            mapping_registry.0
                .get(&msg.target_host_id)
                .or_else(|| {
                    err_writer.write(HostDespawnResponseErrorMsg { 
                        host_id: Some(msg.target_host_id.clone()), 
                        error: Arc::new("Target entity does not exist!".to_string()), 
                        request_key: msg.request_key, 
                    });
                    None
                }
            )
            .map(|entity| HostEntityRemovalTriggered { 
                entity: *entity,
                request_key: msg.request_key,
            })
        }
    );

    msg_writer.write_batch(removals);
}


/// The System counterpart of HostEntityRemovalTriggered Messages. 
/// Handles the core job of that Message - despawning HostMapped entities
pub(crate) fn host_entity_removal_executor<T: HostIdType + 'static> (
    mut msg_reader: MessageReader<HostEntityRemovalTriggered>,
    mut commands: Commands, 
    mut from_hostmapping: ResMut<HostIdToEntityRegistry<T>>,
    mut to_hostmapping: ResMut<EntityToHostIdRegistry<T>>,
    mut success_msg_writer: MessageWriter<HostDespawnResponseSuccessMsg<T>>,
    // TODO: in theory we should have an error message too, but there is no way for this to fail rn
) {
    let msgs = msg_reader.read_with_id().filter_map(|(msg, msg_id)| {
        #[cfg(feature = "logging")]
        bevy::log::debug!(
            "Processing HostEntityRemovalTriggered message (ID: {:?}) - Entity: {:?}",
            msg_id,
            msg.entity
        );

        // Despawn the Entity proper.
        let entity_match = commands
        .get_entity(msg.entity)
        .map(|mut ecmd| { 
            ecmd.despawn()
        });

        match entity_match {
            Err(err) => {
                #[cfg(feature = "logging")]
                bevy::log::error!(
                    "Failed to match an Entity ({:?}) for despawn - {:?}",
                    msg.entity,
                    err,
                );
                None
            },
            Ok(_) => {
                #[cfg(feature = "logging")]
                bevy::log::debug!(
                    "Successfully matched an Entity ({:?}) for despawn...",
                    msg.entity,
                );

                // Remove the deleted Host ID from the Host2Cranium map.
                let host_id = to_hostmapping
                    .0
                    .get(&msg.entity)
                    .map(|hostid| {
                        from_hostmapping.0.remove(hostid);
                        hostid
                    })
                    .cloned();

                // Remove the deleted Host ID from the Cranium2Host map.
                to_hostmapping.0.remove(&msg.entity);

                host_id.and_then(|hostid| {
                    #[cfg(feature = "logging")]
                    bevy::log::debug!(
                        "Successfully removed entity {:?} (HostId: {:?}). Sending success message...",
                        msg.entity,
                        hostid,
                    );
                    Some(HostDespawnResponseSuccessMsg {
                        entity: msg.entity,
                        host_id: Some(hostid),
                        comments: None,
                        request_key: msg.request_key,
                    })
                })
            }
        }
    });

    success_msg_writer.write_batch(msgs);
}


/// Queues up successful Spawn responses as messages to push out to the output channel.
pub(crate) fn forward_despawn_success_signals(
    mut success_messages: MessageReader<HostDespawnResponseSuccessMsg<NativeHostIdType>>,
    mut message_queue: MessageWriter<QueuedApiOutMessage>, 
) {
    let transformed_msgs = success_messages
        .read()
        .map(|msg| {
            let out_msg = cranium_ffi::StagedApiOutMsg::EntityDespawnSuccessful(
                msg.host_id.clone().into(),
                msg.request_key
            );
            #[cfg(feature = "logging")]
            bevy::log::debug!("Queuing up a EntityDespawnSuccessful Message for output channel send: {:?}", out_msg);
            QueuedApiOutMessage(out_msg)
        }
    );
    message_queue.write_batch(transformed_msgs);
}


/// Queues up error Spawn responses as messages to push out to the output channel.
pub(crate) fn forward_despawn_error_signals(
    mut success_messages: MessageReader<HostDespawnResponseErrorMsg<NativeHostIdType>>,
    mut message_queue: MessageWriter<QueuedApiOutMessage>, 
) {
    let transformed_msgs = success_messages
        .read()
        .map(|msg| {
            let out_msg = cranium_ffi::StagedApiOutMsg::EntityDespawnError(
                msg.host_id.clone().into(),
                msg.request_key, 
                msg.error.clone(), 
            );
            #[cfg(feature = "logging")]
            bevy::log::debug!("Queuing up a EntitySpawnError Message for output channel send: {:?}", out_msg);
            QueuedApiOutMessage(out_msg)
        }
    );
    message_queue.write_batch(transformed_msgs);
}
