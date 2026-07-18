/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
#[cfg(feature = "logging")]
use cranium_core::bevy::log::LogPlugin;
use cranium_core::bevy::{app::ScheduleRunnerPlugin, prelude::*};

use crate::helpers::*;


pub struct CraniumTestPlugin; 

impl Plugin for CraniumTestPlugin {
    fn build(&self, app: &mut App) {
        app
        .add_plugins((
            MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(core::time::Duration::from_millis(200))),
            #[cfg(feature = "logging")]
            LogPlugin { 
                level: cranium_core::bevy::log::Level::DEBUG, 
                custom_layer: |_| None, 
                filter: "wgpu=error,bevy_render=info,bevy_ecs=info".to_string(),
                fmt_layer: |_| None,
            }
        ))
        .init_resource::<DespawnedAnyActionTrackers>()
        .add_observer(mark_despawn_occurred)
        .add_systems(
            Last, 
            (
                exit_on_finish_all_tasks,
            ).chain()
        )
        ;
    }
}
