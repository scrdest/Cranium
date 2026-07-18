/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
use cranium_core::bevy::prelude::*;
use cranium_core::*;


/// Helper for triggering AppExit. 
/// If True, we have actually despawned at least one tracker.
/// This is needed so we don't stop BEFORE any trackers are created.
#[derive(Resource)]
pub struct DespawnedAnyActionTrackers(bool);

impl Default for DespawnedAnyActionTrackers {
    fn default() -> Self {
        Self(false)
    }
}

/// Helper for triggering AppExit. 
/// Updates the DespawnedAnyActionTrackers on a despawn.
pub fn mark_despawn_occurred(
    _trigger: On<action_runtime::ActionTrackerDespawnRequested>,
    mut marker: ResMut<DespawnedAnyActionTrackers>
) {
    marker.0 = true;
}

/// Calls AppExit if there are no running tasks left and at least one has been despawned.
/// This is a helper to make it easier to test in a loop without manually killing the app.
pub fn exit_on_finish_all_tasks(
    query: Query<&action_runtime::ActionTracker>,
    despawned: Res<DespawnedAnyActionTrackers>,
    mut exit_writer: MessageWriter<AppExit>,
) {
    if !despawned.0 {
        return;
    }

    if query.is_empty() {
        exit_writer.write(AppExit::Success);
    }
}
