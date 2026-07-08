use bevy::ecs::message::MessageId;
// Needed in-scope for insert_reflect() call
use bevy::ecs::reflect::ReflectCommandExt;

// We need this import or Cargo complains about panic unwinding vOv
use bevy::prelude::*;

use bevy::reflect::{
    serde::TypedReflectDeserializer,
};

use cranium_ffi::{HostIdType, HostMapped, NativeHostIdType};
use serde_json::Value;
use serde::de::DeserializeSeed;

use crate::channels::{HostIdToEntityRegistry, QueuedApiOutMessage};


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
        let Some(component_type) = type_registry.get_with_type_path(&component_path) else {
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
pub struct HostSpawnRequestParams {
    pub components: bevy::platform::collections::HashMap<String, Value>
}

/// This represents queueing up a HostMapped entity for resolution and update/insert of Components
/// based on the results (i.e. - if we already track this Host ID -> update, else insert). 
#[derive(Debug, Message)]
pub struct HostSpawnRequestMsg<I: HostIdType> {
    pub payload: String,
    pub host_id: I,
}

/// Signals a successful Spawn request  - consumed and re-emitted to the output channel as feedback.
#[derive(Debug, Message)]
pub struct HostSpawnResponseSuccessMsg<I: HostIdType> {
    pub entity: Entity,
    pub host_id: I,
    pub comments: Option<String>,
}

/// Signals a bad Spawn request - consumed and re-emitted to the output channel as feedback.
#[derive(Debug, Message)]
pub struct HostSpawnResponseErrorMsg<I: HostIdType> {
    pub error: String,
    pub host_id: I,
}

// Completing the unholy trinity of copypasta, the actual handler using both of the above.
// (https://github.com/bevyengine/bevy/blob/0eac08ae5da33f39d64ad148740c34c14b38c481/crates/bevy_remote/src/builtin_methods.rs#L1908)
// This is the most customized of the three, barely bearing any similarity to the original. 
// Rather than operating on exclusive World access, we use good ol' Commands for this, 
// and simply process Messages (inbound and outbound).
pub fn process_remote_spawn_entity_request<I: HostIdType + 'static>(
    app_type_registry: Res<AppTypeRegistry>,
    real_timer: Res<Time<Real>>, 
    mut mapping_registry: ResMut<HostIdToEntityRegistry<I>>, 
    mut request_stream: MessageReader<HostSpawnRequestMsg<I>>,
    mut success_response_stream: MessageWriter<HostSpawnResponseSuccessMsg<I>>,
    mut error_response_stream: MessageWriter<HostSpawnResponseErrorMsg<I>>,
    mut commands: Commands,
) {
    let type_registry = app_type_registry.read();

    let success_messages = request_stream.read_with_id().filter_map(|(request, request_id)| {
        // IN THEORY we could do all of this in One Big Pipeline with a bunch of and_then()s.
        // However, this is broken down into three logical steps for better legibility.
        bevy::log::debug!(
            "Processing an Entity Spawn Request (RqID: {}, HostId: {:?})...", 
            request_id,
            request.host_id,
        );

        // (1) Deserialize: request payload -> specmap (component key : fields)
        let parsed: Option<HostSpawnRequestParams> = 
            serde_json::from_str(&request.payload)
            .map_err(|err| {
                bevy::log::error!("Error parsing Cranium Entity Spawn Request (RqID: {}, HostId: {:?}) - {}", request_id, request.host_id, err);
                error_response_stream.write(HostSpawnResponseErrorMsg { error: err.to_string(), host_id: request.host_id.clone() });
            }
        ).ok();

        // (2) Deserialize: specmap -> actual Components (dynamic; uses the app type registry)
        let reflected = parsed
        .and_then(|p| Some(p.components))
        .and_then(|comp| {
            deserialize_components(&type_registry, comp)
            .map_err(|e| {
                bevy::log::error!("Error deserializing Components from a Spawn API request (RqID: {}, HostId: {:?}): {:?}", request_id, request.host_id, e); 
                error_response_stream.write(HostSpawnResponseErrorMsg { error: e, host_id: request.host_id.clone() });
            })
            .ok()
        });
        
        // (3) Insert the reflected Components to the Entity and notify success (hopefully)
        let msg = reflected.and_then(|reflect_components| {
            let host_id = Some(&request.host_id);
            let mapped_to_entity = host_id.and_then(|id| mapping_registry.0.get(id));
            let mut is_update = false;

            let maybe_entity = match mapped_to_entity {
                None => {
                    is_update = true;
                    Ok(commands.spawn_empty())
                },
                Some(&e) => commands.get_entity(e).map_err(
                    |err| {
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
                    mapping_registry.0.insert(request.host_id.clone(), entity_id);
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
                    true => bevy::log::error!(
                        "Error updating Components from a Spawn API request (RqID: {}, HostId {:?}): {:?}", 
                        request_id, 
                        request.host_id,
                        e
                    ),
                    false => bevy::log::error!(
                        "Error inserting Components from a Spawn API request (RqID: {}, HostId {:?}): {:?}", 
                        request_id, 
                        request.host_id,
                        e
                    )
                }
                error_response_stream.write(HostSpawnResponseErrorMsg { error: e, host_id: request.host_id.clone() });
            })
            .ok()
            .and_then(|_| {
                match is_update {
                    false => bevy::log::debug!(
                        "Successfully spawned an Entity ({}, HostId {:?}) with Components based on Spawn request (RqID: {})", 
                        entity_id, 
                        request.host_id,
                        request_id,
                    ),
                    true => bevy::log::debug!(
                        "Successfully updated an Entity ({}, HostId {:?}) with Components based on Spawn request (RqID: {})", 
                        entity_id, 
                        request.host_id,
                        request_id,
                    ),
                }
                
                Some(HostSpawnResponseSuccessMsg {
                    entity: entity_id, 
                    host_id: request.host_id.clone(), 
                    comments: None,
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
        .map(|(msg)| {
            QueuedApiOutMessage(cranium_ffi::ApiOutMsg::EntitySpawnSuccessful(msg.host_id.clone()))
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
        .map(|(msg)| {
            QueuedApiOutMessage(cranium_ffi::ApiOutMsg::EntitySpawnError(msg.host_id.clone(), msg.error.clone()))
        }
    );
    message_queue.write_batch(transformed_msgs);
}
