/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
use core::time::Duration;
use core::{sync::atomic};

use cranium_core::bevy::prelude::*;
use cranium_core::bevy::log;

use cranium_bevy_plugin::CraniumPlugin;
use cranium_ffi::{
    ApiInMsg, ApiOutMsg, EntityOperation, NativeHostIdType, RequestKey, 
    ffi_trait::*,
    FFISpawnRequestBatch,
    FFIDespawnRequestBatch,
};

use crate::channels;
use crate::heartbeat::SHOULD_HEARTBEAT;
use crate::heartbeat::AutoRunHeartbeatPlugin;

/// Triggers AutoRunHeartbeat events, keeping the AutoRun-ing Cranium instance alive.
/// This function is expected to be called periodically by the user from downstream code 
/// as an alternative to driving the whole App themselves.
pub fn request_heartbeat() {
    SHOULD_HEARTBEAT.store(true, atomic::Ordering::Release);
}

pub fn create_app() -> App {
    let mut app = App::new();
    app.add_plugins(CraniumPlugin);

    #[cfg(feature = "logging")]
    let log_level = option_env!("CRANIUM_LOG_LEVEL")
        .and_then(|s| {
            match s {
                "DEBUG" => Some(log::Level::DEBUG),
                "INFO" => Some(log::Level::INFO),
                "WARN" => Some(log::Level::WARN),
                "ERROR" => Some(log::Level::ERROR),
                _ => None
            }
        })
        .unwrap_or(log::Level::INFO);

    #[cfg(feature = "logging")]
    app.add_plugins(
        log::LogPlugin { 
            level: log_level, 
            custom_layer: |_| None, 
            filter: "wgpu=error,bevy_render=info,bevy_ecs=info".to_string(),
            fmt_layer: |_| None,
        }
    );
    app
}

pub fn _tick_world(app: &mut App) -> &mut App {
    app.update();
    app
}

pub fn configure_for_autorun(mut app: App) -> App {
    let run_rate = option_env!("CRANIUM_AUTORUN_RATE_MILLISECONDS")
        .map(|s| s.trim().parse::<u64>().ok()).flatten()
        .unwrap_or(200) // 200ms by default
    ; 

    app.add_plugins((
        MinimalPlugins.set(cranium_core::bevy::app::ScheduleRunnerPlugin::run_loop(core::time::Duration::from_millis(run_rate))),
        AutoRunHeartbeatPlugin,
        crate::plugin::ApiChannelsPlugin::<NativeHostIdType>::with_default_bounds(),
    ));
    
    app
}

pub fn autorun(mut app: App) {
    app
    .run();
}

pub fn create_and_autorun() {
    let app = configure_for_autorun(create_app());
    #[cfg(feature = "logging")]
    log::info!("Created a Cranium Server app, running...");
    autorun(app);
}

pub fn shutdown() -> bool {
    let success = channels::IN_CHANNEL.get().map(|ch| {
        ch.send(ApiInMsg::Shutdown).err()
    })
    .flatten()
    .map(|e| {
        #[cfg(feature = "logging")]
        log::error!("Cranium channel send error: {}", e);
        e
    })
    .is_none()
    ;

    success
}

pub fn await_message() -> Option<ApiOutMsg> {
    let msg = channels::OUT_CHANNEL.get().map(|ch| {
        ch.recv().ok()
    })
    .flatten()
    .map(|m| m.into())
    ;

    msg
}

pub fn try_get_message_with_timeout(timeout_milliseconds: u32) -> Option<ApiOutMsg> {
    let msg = channels::OUT_CHANNEL.get().map(|ch| {
        ch.recv_timeout(Duration::from_millis(timeout_milliseconds.into())).ok()
    })
    .flatten()
    .map(|m| m.into())
    ;

    msg
}

pub fn try_get_message_with_default_timeout() -> Option<ApiOutMsg> {
    try_get_message_with_timeout(1000)
}

pub fn write_ping() -> bool {
    let success = channels::IN_CHANNEL.get().map(|ch| {
        ch.send(ApiInMsg::Ping).err()
    })
    .flatten()
    .map(|e| {
        #[cfg(feature = "logging")]
        log::error!("Cranium channel send error: {}", e);
        e
    })
    .is_none()
    ;

    success
}

pub fn request_spawn<I: Into<NativeHostIdType>>(
    host_id: I, 
    components: cranium_ffi::FFIIngestedString, 
    request_key: RequestKey,
) -> bool {
    // As a UX convenience, spawning entities is treated as a heartbeat signal too.
    SHOULD_HEARTBEAT.store(true, atomic::Ordering::Release);

    let ops = vec![
        EntityOperation::UpsertEntity { 
            host_id: host_id.into(), 
            components: components,
            request_key: request_key,
        }
    ];

    let success = channels::IN_CHANNEL.get().map(|ch| {
        ch.send(ApiInMsg::SyncBatch { 
            ops: ops
        }).err()
    })
    .flatten()
    .map(|e| {
        #[cfg(feature = "logging")]
        log::error!("Cranium channel send error: {}", e);
        e
    })
    .is_none()
    ;

    success
}

pub fn request_despawn<I: Into<NativeHostIdType>>(
    host_id: I,
    request_key: RequestKey,
) -> bool {
    // As a UX convenience, despawning entities is treated as a heartbeat signal too.
    SHOULD_HEARTBEAT.store(true, atomic::Ordering::Release);

    let ops = vec![
        EntityOperation::RemoveEntity { 
            host_id: host_id.into(), 
            request_key: request_key,
        }
    ];

    let success = channels::IN_CHANNEL.get().map(|ch| {
        ch.send(ApiInMsg::SyncBatch { 
            ops: ops
        }).err()
    })
    .flatten()
    .map(|e| {
        #[cfg(feature = "logging")]
        log::error!("Cranium channel send error: {}", e);
        e
    })
    .is_none()
    ;

    success
}

/// Handles spawning/updating data for a batch of input HostIDs. 
/// 
/// Returns a u64 signalling how many items in the batch had been successfully processed. 
/// 
/// In a normal happy-path case, this should be the same as the length of the input batch.
/// 
/// In case of a syntax error in the request, this will be the count of items up until the 
/// first failed item, and no further items in the batch will be processed (and therefore, 
/// the returned u64 can be used to find out the index of the first problematic batch member).
/// 
/// In case of a message send error, the returned value will be 0 to signal none of the members 
/// of the batch will have been processed (as they will never have made it to the actual systems 
/// that handle the processing, since the message pipe has failed).
/// 
/// Note that the returned value is a *count*, not an index (i.e. it's 1-based, not 0-based)!
pub fn request_spawn_batch<I: Into<NativeHostIdType> + TriviallyFFIReadable + Clone>(
    batch: FFISpawnRequestBatch<I>, 
    request_key: RequestKey,
) -> u64 {
    // As a UX convenience, spawning entities is treated as a heartbeat signal too.
    SHOULD_HEARTBEAT.store(true, atomic::Ordering::Release);

    let mut successful = true;
    let mut last_processed_idx = 0u64;

    let ops = Vec::from_iter(
        batch.iter().enumerate().filter_map(|(idx, inp)| {
            match successful {
                false => {
                    None
                }, 
                
                true => {
                    last_processed_idx = (1+idx) as u64;

                    match unsafe { 
                        inp.components.try_ffi_read_unsafe() 
                    } {
                        Ok(components) => {
                            Some(EntityOperation::UpsertEntity { 
                                host_id: inp.host_id.clone().into(), 
                                components: components, 
                                request_key 
                            })
                        }
                        Err(e) => {
                            #[cfg(feature = "logging")]
                            log::error!("Error parsing Components spec {:?} for request_spawn_batch - {:?}",
                                inp.components,
                                e
                            );
                            successful = false;
                            None
                        }
                    }
                }
            }
        })
    );

    let sent = {
        channels::IN_CHANNEL.get().map(|ch| {
            ch.send(ApiInMsg::SyncBatch { 
                ops: ops
            }).err()
        })
        .flatten()
        .map(|e| {
            #[cfg(feature = "logging")]
            log::error!("Cranium channel send error: {}", e);
            e
        })
        .is_none()
    };

    match sent {
        false => 0, // if nothing was sent, then we effectively processed zero elements
        true => last_processed_idx
    }
}

/// Handles despawning Entities for a batch of input HostIDs. 
/// 
/// Returns a u64 signalling how many items in the batch had been successfully processed. 
/// 
/// In a normal happy-path case, this should be the same as the length of the input batch.
/// 
/// In case of a syntax error in the request, this will be the count of items up until the 
/// first failed item, and no further items in the batch will be processed (and therefore, 
/// the returned u64 can be used to find out the index of the first problematic batch member).
/// 
/// In case of a message send error, the returned value will be 0 to signal none of the members 
/// of the batch will have been processed (as they will never have made it to the actual systems 
/// that handle the processing, since the message pipe has failed).
/// 
/// Note that the returned value is a *count*, not an index (i.e. it's 1-based, not 0-based)!
pub fn request_despawn_batch<I: Into<NativeHostIdType> + TriviallyFFIReadable + Clone>(
    batch: FFIDespawnRequestBatch<I>, 
    request_key: RequestKey,
) -> u64 {
    // As a UX convenience, spawning entities is treated as a heartbeat signal too.
    SHOULD_HEARTBEAT.store(true, atomic::Ordering::Release);

    let ops = Vec::from_iter(
        batch.iter().map(|inp| {
                EntityOperation::RemoveEntity { 
                    host_id: inp.host_id.clone().into(), 
                    request_key 
                }
            }
        )
    );

    let ops_size = ops.len();

    let sent = channels::IN_CHANNEL.get().map(|ch| {
        ch.send(ApiInMsg::SyncBatch { 
            ops: ops
        }).err()
    })
    .flatten()
    .map(|e| {
        #[cfg(feature = "logging")]
        log::error!("Error sending a FFIDespawnRequestBatch - {:?}", e);
        e
    })
    .is_none()
    ;

    match sent {
        false => 0,
        true => ops_size as u64
    }
}

pub fn request_decision<I: Into<NativeHostIdType>>(host_id: I, request_key: RequestKey) -> bool {
    // As a UX convenience, querying AIs is treated as a heartbeat signal too.
    SHOULD_HEARTBEAT.store(true, atomic::Ordering::Release);

    let ops = vec![
        (request_key, host_id.into())
    ];

    let success = channels::IN_CHANNEL.get().map(|ch| {
        ch.send(ApiInMsg::RequestDecision { targets: ops }).err()
    })
    .flatten()
    .map(|e| {
        #[cfg(feature = "logging")]
        log::error!("Cranium channel send error: {}", e);
        e
    })
    .is_none()
    ;

    success
}

fn request_spawn_logic<I: Into<NativeHostIdType>>(
    host_id: I, 
    components: cranium_ffi::FFIRawString,
    request_key: RequestKey,
) -> bool {
    match unsafe { 
        // SAFETY: Invariants must be upheld by the C API users.
        //         See docs on core::ffi::CStr::from_ptr() for a full list.
        components.try_ffi_read_unsafe() 
    } {
        Ok(safe_components) => {
            request_spawn(host_id, safe_components, request_key)
        },
        Err(e) => {
            #[cfg(feature = "logging")]
            log::error!("Error parsing Components spec {:?} for request_spawn_logic - {:?}",
                components,
                e
            );
            false
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_create_and_autorun() {
    create_and_autorun();
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_shutdown() {
    shutdown();
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_keepalive() {
    request_heartbeat();
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_await_message() -> cranium_ffi::FFIOption<ApiOutMsg> {
    await_message().into()
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_try_get_message_with_timeout(timeout_milliseconds: u32) -> cranium_ffi::FFIOption<ApiOutMsg> {
    try_get_message_with_timeout(timeout_milliseconds).into()
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_try_get_message_with_default_timeout() -> cranium_ffi::FFIOption<ApiOutMsg> {
    try_get_message_with_default_timeout().into()
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_write_ping() -> bool {
    write_ping()
}


// One of the great joys of C is that we have no generics, so we need to implement a wrapper function 
// for every single one of the types that we want to support. This macro automates that somewhat, making 
// it easier to add new supported types or functions in the future.
// 
// It currently requires writing out the names of all the implemented functions, as I couldn't be bothered 
// to add in `paste` as a dependency and I kind of like the explicitness. 
macro_rules! impl_ffi_methods_for_type {
    (
        $spawn_name:ident, 
        $despawn_name:ident, 
        $spawn_batch_name:ident, 
        $despawn_batch_name:ident, 
        $decision_name:ident, 
        $ty:ty
    ) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $spawn_name(host_id: $ty, components: cranium_ffi::FFIRawString, request_key: RequestKey) -> bool {
            request_spawn_logic(host_id, components, request_key)
        }
        
        #[unsafe(no_mangle)]
        pub extern "C" fn $despawn_name(host_id: $ty, request_key: RequestKey) -> bool {
            request_despawn(host_id, request_key)
        }
        
        #[unsafe(no_mangle)]
        pub extern "C" fn $spawn_batch_name(batch: FFISpawnRequestBatch<$ty>, request_key: RequestKey) -> u64 {
            request_spawn_batch(batch, request_key)
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn $despawn_batch_name(batch: FFIDespawnRequestBatch<$ty>, request_key: RequestKey) -> u64 {
            request_despawn_batch(batch, request_key)
        }
        
        #[unsafe(no_mangle)]
        pub extern "C" fn $decision_name(host_id: $ty, request_key: RequestKey) -> bool {
            request_decision(host_id, request_key)
        }
    };
}

impl_ffi_methods_for_type!(
    cranium_request_spawn_u64, 
    cranium_request_despawn_u64, 
    cranium_request_spawn_batch_u64, 
    cranium_request_despawn_batch_u64, 
    cranium_request_decision_u64, 
    u64
);

impl_ffi_methods_for_type!(
    cranium_request_spawn_u32, 
    cranium_request_despawn_u32, 
    cranium_request_spawn_batch_u32, 
    cranium_request_despawn_batch_u32, 
    cranium_request_decision_u32, 
    u32
);

impl_ffi_methods_for_type!(
    cranium_request_spawn_i64, 
    cranium_request_despawn_i64, 
    cranium_request_spawn_batch_i64, 
    cranium_request_despawn_batch_i64, 
    cranium_request_decision_i64, 
    i64
);

impl_ffi_methods_for_type!(
    cranium_request_spawn_i32, 
    cranium_request_despawn_i32, 
    cranium_request_spawn_batch_i32, 
    cranium_request_despawn_batch_i32, 
    cranium_request_decision_i32, 
    i32
);
