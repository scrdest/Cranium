/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
use core::time::Duration;
use core::{sync::atomic};

use bevy::prelude::*;

use cranium_bevy_plugin::CraniumPlugin;
use cranium_ffi::{ApiInMsg, ApiOutMsg, EntityOperation, NativeHostIdType};

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
    app.add_plugins(
        bevy::log::LogPlugin { 
            level: bevy::log::Level::DEBUG, 
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
    let run_rate = option_env!("CORTEX_AUTORUN_RATE_MILISECONDS")
        .map(|s| s.trim().parse::<u64>().ok()).flatten()
        .unwrap_or(200) // 200ms by default
    ; 

    app.add_plugins((
        MinimalPlugins.set(bevy::app::ScheduleRunnerPlugin::run_loop(core::time::Duration::from_millis(run_rate))),
        AutoRunHeartbeatPlugin,
        crate::plugin::ApiChannelsPlugin::default(),
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
    bevy::log::info!("Created a Cranium Server app, running...");
    autorun(app);
}

pub fn await_message() -> Option<ApiOutMsg> {
    let msg = channels::OUT_CHANNEL.get().map(|ch| {
        ch.recv().ok()
    }).flatten();

    msg
}

pub fn try_get_message_with_timeout() -> Option<ApiOutMsg> {
    let msg = channels::OUT_CHANNEL.get().map(|ch| {
        ch.recv_timeout(Duration::from_secs(1)).ok()
    }).flatten();

    msg
}

pub fn write_ping() -> bool {
    let success = channels::IN_CHANNEL.get().map(|ch| {
        ch.send(ApiInMsg::Ping).err()
    }).flatten().is_none();

    success
}

pub fn request_spawn<I: Into<NativeHostIdType>>(host_id: I, components: cranium_ffi::FFIIngestedString) -> bool {
    // As a UX convenience, spawning entities is treated as a heartbeat signal too.
    SHOULD_HEARTBEAT.store(true, atomic::Ordering::Release);

    let component_string = components;

    let ops = vec![
        EntityOperation::UpsertEntity { 
            host_id: host_id.into(), 
            components: component_string
        }
    ];

    let success = channels::IN_CHANNEL.get().map(|ch| {
        ch.send(ApiInMsg::SyncBatch { 
            ops: ops
        }).err()
    }).flatten().is_none();

    success
}

pub fn request_despawn<I: Into<NativeHostIdType>>(host_id: I) -> bool {
    // As a UX convenience, despawning entities is treated as a heartbeat signal too.
    SHOULD_HEARTBEAT.store(true, atomic::Ordering::Release);

    let ops = vec![
        EntityOperation::RemoveEntity { 
            host_id: host_id.into(), 
        }
    ];

    let success = channels::IN_CHANNEL.get().map(|ch| {
        ch.send(ApiInMsg::SyncBatch { 
            ops: ops
        }).err()
    }).flatten().is_none();

    success
}

pub fn request_decision<I: Into<NativeHostIdType>>(host_id: I, request_key: cranium_ffi::FFIIngestedString) -> bool {
    // As a UX convenience, querying AIs is treated as a heartbeat signal too.
    SHOULD_HEARTBEAT.store(true, atomic::Ordering::Release);

    let string_cast = request_key;

    let ops = vec![
        (string_cast, host_id.into())
    ];

    let success = channels::IN_CHANNEL.get().map(|ch| {
        ch.send(ApiInMsg::RequestDecision { targets: ops }).err()
    }).flatten().is_none();

    success
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_create_and_autorun() {
    create_and_autorun();
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
pub extern "C" fn cranium_try_get_message_with_timeout() -> cranium_ffi::FFIOption<ApiOutMsg> {
    try_get_message_with_timeout().into()
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_write_ping() -> bool {
    write_ping()
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_request_spawn_u64(host_id: u64, components: cranium_ffi::FFIRawString) -> bool {
    request_spawn(host_id, unsafe { cranium_ffi::ingest_string_from_ffi_raw_string(components) })
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_request_spawn_usize(host_id: usize, components: cranium_ffi::FFIRawString) -> bool {
    request_spawn(host_id, unsafe { cranium_ffi::ingest_string_from_ffi_raw_string(components) })
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_request_despawn_u64(host_id: u64) -> bool {
    request_despawn(host_id)
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_request_despawn_usize(host_id: usize) -> bool {
    request_despawn(host_id)
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_request_decision_u64(host_id: u64, request_key: cranium_ffi::FFIRawString) -> bool {
    request_decision(host_id, unsafe { cranium_ffi::ingest_string_from_ffi_raw_string(request_key) })
}

#[unsafe(no_mangle)]
pub extern "C" fn cranium_request_decision_usize(host_id: usize, request_key: cranium_ffi::FFIRawString) -> bool {
    request_decision(host_id, unsafe { cranium_ffi::ingest_string_from_ffi_raw_string(request_key) })
}
