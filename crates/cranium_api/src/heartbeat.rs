/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! Heartbeat/keepalive logic.
//! 
//! In AI Server mode with autorun, Cranium runs as a sidecar to some other application. 
//! This will most likely be done in some flavor of backgrounding solution, e.g. a thread or a subprocess. 
//! 
//! We cannot guarantee that the host application will always kill Cranium when the host dies in all such 
//! cases. Forcing the user to implement a shutdown mechanism for the AI integration would be pretty annoying, 
//! so instead Cranium comes with a fallback of its own.
//! 
//! When AutoRunHeartbeatPlugin is added to the app, Bevy will periodically check a stored timestamp ('heartbeat'). 
//! If the timestamp is too old, the app will consider it a sign that it has been abandoned and exit. 
//! The timestamp can be updated by calling the [`cranium_keepalive`] function in the FFI.
//! 
//! The timeout for missed heartbeats can be configured at compile-time 
//! by setting the `CORTEX_AUTORUN_HEARTBEAT_TIMEOUT_SECONDS` envvar.
//! 
//! For long-running applications, numerical stability is ensured by using a wrap period for the heartbeat time, 
//! which is also configurable, using `CORTEX_AUTORUN_PERIOD_SECONDS` - generally, this should be at least an order 
//! of magnitude or two higher than the timeout time or you may observe false negatives.

use core::{num::NonZero, sync::atomic, time::Duration};
use bevy::{prelude::*};


pub(crate) static SHOULD_HEARTBEAT: atomic::AtomicBool = atomic::AtomicBool::new(false);

/// This is a resource that emulates needing mutable access to SHOULD_HEARTBEAT.
/// We're using this to trick Bevy into scheduling systems that mutate SHOULD_HEARTBEAT properly.
#[derive(Resource, Default)]
pub(crate) struct ShouldHeartbeatAccessToken;


#[derive(Resource)]
struct AutoRunHeartbeatTimeout(core::time::Duration);

impl Default for AutoRunHeartbeatTimeout {
    fn default() -> Self {
        Self(core::time::Duration::from_mins(2))
    }
}

#[derive(Resource)]
struct AutoRunHeartbeatWrapPeriod(core::time::Duration);

impl Default for AutoRunHeartbeatWrapPeriod {
    fn default() -> Self {
        Self(core::time::Duration::from_hours(6))
    }
}


#[derive(Default, Resource)]
struct AutoRunHeartbeatTracker {
    last_tick: core::time::Duration
}

/// 
#[derive(Event)]
struct AutoRunHeartbeat;


/// The actual system responsible for making heartbeats happen.
/// It checks a static boolean (atomic) and runs only if True. 
/// Resets the value back to False after running.
fn handle_heartbeat(
    _token: ResMut<ShouldHeartbeatAccessToken>,
    mut commands: Commands,
) {
    let should_run = SHOULD_HEARTBEAT.load(atomic::Ordering::Acquire);
    if should_run {
        SHOULD_HEARTBEAT.store(false, atomic::Ordering::Release);
        commands.trigger(AutoRunHeartbeat);
    }
    
}

fn update_heartbeat(
    _trigger: On<AutoRunHeartbeat>,
    timer: Res<Time<Real>>,
    mut heartbeat_tracker: ResMut<AutoRunHeartbeatTracker>
) {
    let now = timer.elapsed_wrapped();
    heartbeat_tracker.last_tick = now;
}

fn setup_wrap_period(
    wrap_period: Res<AutoRunHeartbeatWrapPeriod>,
    mut timer: ResMut<Time<Real>>,
) {
    let period = wrap_period.0;
    timer.set_wrap_period(period);
}

/// A System that checks for keep-alive messages (heartbeats) and kills the app if none were received.
fn check_heartbeat_system(
    heartbeat_tracker: Res<AutoRunHeartbeatTracker>,
    heartbeat_timeout: Res<AutoRunHeartbeatTimeout>,
    timer: Res<Time<Real>>,
    mut app_exit: MessageWriter<AppExit>,
) {
    let timeout = heartbeat_timeout.0;
    let last_tick = heartbeat_tracker.last_tick;

    let now = timer.elapsed_wrapped();
    let now = match now < last_tick {
        // We need to ensure that wrapping doesn't cause issues.
        //
        // If the delta would be negative, that means the last heartbeat 
        // happened in the previous wrap period (or more, we can't tell).
        // We'll readd it to get a more realistic, positive delta value.
        //
        // Note that if someone set timeout >> wrap, the timeout will NEVER happen!
        false => now,
        true => {
            now + timer.wrap_period()
        },
    };

    let delta = now - last_tick;

    if delta > timeout {
        bevy::log::error!(
            "Cranium received no heartbeat in more than {:?}s (delta:{:?}s, last update time: {:?}s), quitting!", 
            timeout.as_secs(), delta.as_secs(), last_tick.as_secs()
        );
        app_exit.write(AppExit::Error(NonZero::new(1u8).unwrap()));
    };
}

pub struct AutoRunHeartbeatPlugin;

impl Plugin for AutoRunHeartbeatPlugin {
    fn build(&self, app: &mut App) {
        let timeout_seconds = option_env!("CORTEX_AUTORUN_HEARTBEAT_TIMEOUT_SECONDS")
        .map(|s| s.trim().parse::<u64>().ok()).flatten()
        .unwrap_or(60*2) // 2 mins by default
        ; 

        let period_seconds = option_env!("CORTEX_AUTORUN_PERIOD_SECONDS")
            .map(|s| s.trim().parse::<u64>().ok()).flatten()
            .unwrap_or(60*60*6) // 6 hours by default
        ; 

        app
        .init_resource::<AutoRunHeartbeatTracker>()
        .init_resource::<ShouldHeartbeatAccessToken>()
        .insert_resource(AutoRunHeartbeatTimeout(Duration::from_secs(timeout_seconds)))
        .insert_resource(AutoRunHeartbeatWrapPeriod(Duration::from_secs(period_seconds)))
        .add_systems(Startup, setup_wrap_period)
        .add_systems(First, handle_heartbeat)
        .add_systems(Last, check_heartbeat_system)
        .add_observer(update_heartbeat)
        ;
    }
}
