/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use crossbeam_channel;
use cranium_ffi::{ApiInMsg, ApiOutMsg, NativeHostIdType};

use crate::spawn;
use crate::channels::*;
use crate::spawn::host_entity_removal_executor;
use crate::spawn::host_entity_removal_request_processor;

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

        // Same, but for ActionKeys rather than Entities
        let host_action_id_map: HostActionIdMap<NativeHostIdType> = HostActionIdMap {
            key_to_host_id_map: HashMap::new(),
            host_id_to_key_map: HashMap::new(),
        };
        app.insert_resource(host_action_id_map);

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
        app.add_systems(PreUpdate, host_entity_removal_request_processor::<NativeHostIdType>);
        
        app.add_message::<HostEntityRemovalTriggered>();
        app.add_message::<HostEntityRequestRemovalMessage<NativeHostIdType>>();
        app.add_message::<spawn::HostDespawnResponseSuccessMsg<NativeHostIdType>>();
        app.add_message::<spawn::HostDespawnResponseErrorMsg<NativeHostIdType>>();

        app.add_systems(
            Last, 
            (
                spawn::host_entity_removal_executor::<NativeHostIdType>,
                (
                    spawn::forward_despawn_error_signals,
                    spawn::forward_despawn_success_signals,
                )
            ).chain()
        );

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
        app.add_message::<cranium_core::events::NoDecisionMessage>();
        app.add_systems(PreUpdate, decision_requested_msg_handler::<NativeHostIdType>);
        app.add_observer(decision_output_handler::<NativeHostIdType>);
        app.add_systems(PostUpdate, decision_failed_handler::<NativeHostIdType>);

        // Add common type registrations
        // TODO: Consider making this opt-in only.
        app.add_plugins(crate::type_registrations::CraniumApiTypesRegistrationPlugin);
    }
}
