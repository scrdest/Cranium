/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

use core::marker::PhantomData;

use cranium_core::bevy::platform::collections::HashMap;
use cranium_core::bevy::platform::collections::HashSet;
use cranium_core::bevy::prelude::*;
use crossbeam_channel;
use cranium_ffi::{ApiInMsg, StagedApiOutMsg, HostIdType, NativeHostIdType};

use crate::spawn;
use crate::channels::*;


/// A Plugin that adds Channels for communication between Bevy worlds and external code.
#[derive(Default)]
pub struct ApiChannelsPlugin<I: HostIdType> {
    in_channel_bound: Option<usize>,
    out_channel_bound: Option<usize>,
    host_id_type: PhantomData<I>,
}

impl<I: HostIdType> ApiChannelsPlugin<I> {
    // This is roughly equivalent to a P::default() call, but if the generic 'I' 
    // type does not implement Default (and NativeHostId does not, for example), 
    // we cannot use P::default() for the job. 
    pub fn with_default_bounds() -> Self {
        Self {
            in_channel_bound: None,
            out_channel_bound: None,
            host_id_type: PhantomData,
        }
    }

    pub fn with_bounds(in_bound: Option<usize>, out_bound: Option<usize>) -> Self {
        Self {
            in_channel_bound: in_bound,
            out_channel_bound: out_bound,
            host_id_type: PhantomData,
        }
    }

    pub fn with_bounds_tuple(bounds: Option<(usize, usize)>) -> Self {
        Self {
            in_channel_bound: bounds.map(|b| b.0),
            out_channel_bound: bounds.map(|b| b.1),
            host_id_type: PhantomData,
        }
    }
}


// TODO: Remove the Into<NativeHostIdType> bound - currently needed because FFI types require NativeHostIdType.
impl<I: HostIdType + 'static + Into<NativeHostIdType>> Plugin for ApiChannelsPlugin<I> {
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
        let (out_snd, out_rcv) = crossbeam_channel::bounded::<StagedApiOutMsg>(out_bound);
        let channel_result = OUT_CHANNEL.set(out_rcv.clone());
        
        channel_result.expect("ApiChannelsPlugin OUT_CHANNEL is already initialized (somehow)!");

        let start_sent = out_snd.send(StagedApiOutMsg::CraniumStarted);
        start_sent.expect("Failed to send initial message on the OUT channel.");
        
        // Resources for handling output channel stuff.
        app.insert_resource(ApiOutputChannel { sender: out_snd });
        app.insert_resource(ApiOutputChannelMaintenance { receiver: out_rcv, pop_messages: true });

        // Resources for mapping Host IDs to Cranium IDs and back.
        let from_host_id_reg: HashMap<I, Entity> = HashMap::new();
        let to_host_id_reg: HashMap<Entity, I> = HashMap::new();
        app.insert_resource(HostIdToEntityRegistry(from_host_id_reg));
        app.insert_resource(EntityToHostIdRegistry(to_host_id_reg));

        // Same, but for ActionKeys rather than Entities
        let host_action_id_map: HostActionIdMap<I> = HostActionIdMap {
            key_to_host_id_map: HashMap::new(),
            host_id_to_key_map: HashMap::new(),
        };
        app.insert_resource(host_action_id_map);

        // Stash for FFI-output Arc<String>s to avoid use-after-free by consumers or memleaking.
        app.insert_resource(SentMessageStore { stash: HashMap::new(), insert_time: HashSet::new() });
        
        // Authomated cleanup for the FFI stash. 
        // Cranium will not *guarantee* the FFI strings are valid past 1 hr from insertion.
        app.add_systems(Last, stashed_message_housekeeping::<3600>);

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
                process_queued_output_messages::<10>,
            ).chain()
        );

        // Entity removal core pipeline.
        // 
        // Note that those are ultimately processors on a Message bus - by design, you can hook up 
        // additional consumer Systems to these buses to add additional handling for Host entity removals. 
        app.add_systems(PreUpdate, spawn::host_entity_removal_request_processor::<I>);
        
        app.add_message::<HostEntityRemovalTriggered>();
        app.add_message::<HostEntityRequestRemovalMessage<I>>();
        app.add_message::<spawn::HostDespawnResponseSuccessMsg<I>>();
        app.add_message::<spawn::HostDespawnResponseErrorMsg<I>>();

        app.add_systems(
            Last, 
            (
                spawn::host_entity_removal_executor::<I>,
                (
                    spawn::forward_despawn_error_signals,
                    spawn::forward_despawn_success_signals,
                )
            ).chain()
        );

        // Entity upsert core pipeline. 
        // 
        // Note that those are ultimately processors on a Message bus - by design, you can hook up 
        // additional consumer Systems to these buses to add additional handling for Host entity upserts. 
        app.add_message::<spawn::HostSpawnRequestMsg<I>>();
        app.add_message::<spawn::HostSpawnResponseSuccessMsg<I>>();
        app.add_message::<spawn::HostSpawnResponseErrorMsg<I>>();
        app.add_systems(
            PreUpdate, 
            (
                spawn::process_remote_spawn_entity_request::<I>,
                (
                    spawn::forward_spawn_error_signals,
                    spawn::forward_spawn_success_signals,
                )
            ).chain()
        );

        // Async decision request/response handling.
        app.add_message::<DecisionRequestedMsg::<I>>();
        app.add_message::<cranium_core::events::NoDecisionMessage>();
        app.add_systems(PreUpdate, decision_requested_msg_handler::<I>);
        app.add_systems(PostUpdate, decision_failed_handler::<I>);
        app.add_observer(decision_output_handler::<I>);

        // Add common type registrations
        // TODO: Consider making this opt-in only.
        app.add_plugins(crate::type_registrations::CraniumApiTypesRegistrationPlugin);
    }
}
