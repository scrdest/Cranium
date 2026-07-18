/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

use cranium_core::bevy::prelude::*;
use cranium_core::actions;
use cranium_core::action_runtime;
use cranium_core::action_state;
use cranium_core::considerations;
use cranium_core::context_fetchers;
use cranium_core::decision_loop;
use cranium_core::smart_object;

#[cfg(feature = "include_actionset_loader")]
use cranium_actionset_loader::ActionSetAssetPlugin;

#[cfg(feature = "ai_server")]
use cranium_core::bevy::remote::RemotePlugin;

pub struct CraniumPlugin; 

impl Plugin for CraniumPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "include_actionset_loader")]
        app
        .add_plugins((
            ActionSetAssetPlugin, 
        ));

        #[cfg(feature = "ai_server")]
        app
        .add_plugins((
            RemotePlugin, 
        ));

        app
        .add_plugins((
            actions::ActionHandlerPlugin,
            action_state::ActionStateUpdatesPlugin,
            context_fetchers::ContextFetcherPlugin, 
            considerations::ConsiderationPlugin,
        ))
        .init_resource::<action_runtime::UserDefaultActionTrackerSpawnConfig>()
        .init_resource::<smart_object::ActionSetStore>()
        .add_message::<cranium_core::events::AiActionDispatchToUserCode>()
        .add_observer(action_runtime::create_tracker_for_picked_action)
        .add_observer(action_runtime::actiontracker_triggered_spawner)
        .add_observer(action_runtime::actiontracker_triggered_despawner)
        .add_observer(decision_loop::prepare_ai)
        .add_observer(decision_loop::decision_engine)
        // .add_observer(decision_loop::trigger_dispatch_to_user_actions)
        .add_systems(
            FixedPostUpdate, 
            (
                decision_loop::handle_dispatch_to_user_actions,
                action_runtime::actiontracker_done_cleanup_system,
            ).chain()
        )
        ;
    }
}
