use core::borrow::Borrow;

use bevy::prelude::*;
use bevy::{platform::sync::{OnceLock}};
use crossbeam_channel;
use cranium_core::types::{ApiInMsg, ApiOutMsg};

const DEFAULT_IN_CHANNEL_BOUND: usize = 100;
const DEFAULT_OUT_CHANNEL_BOUND: usize = 100;
const MAX_FULL_TICKS_FOR_MAINTENANCE: usize = 10;


/// The Channel that routes messages FROM the host TO the Cranium Bevy App. 
/// 
/// If Cranium is our 'brain', this would be our 'afferent nerves'.
/// 
/// This is used e.g. to sync state updates from the user to the AI engine.
pub(crate) static IN_CHANNEL: OnceLock<crossbeam_channel::Sender<ApiInMsg>> = OnceLock::new();


/// The Channel that routes messages FROM Cranium back TO the user. 
/// 
/// If Cranium is our 'brain', this would be our 'efferent nerves'.
/// 
/// This is used to exfiltrate data such as chosen Actions out of the AI engine and into the host.
pub(crate) static OUT_CHANNEL: OnceLock<crossbeam_channel::Receiver<ApiOutMsg>> = OnceLock::new();


/// A Resource that represents Read access to the Input channel, 
/// i.e. accessing messages sent BY the host application TO Cranium. 
#[derive(Resource)]
pub struct ApiInputChannel {
    pub receiver: crossbeam_channel::Receiver<ApiInMsg>
}


/// A Resource that represents Write access to the Output channel, 
/// i.e. messages sent BY Cranium TO the host application.
#[derive(Resource)]
pub struct ApiOutputChannel {
    pub sender: crossbeam_channel::Sender<ApiOutMsg>
}


/* --  WARNING:  --
Both Resources below are not meant for public consumption; 
they are specialized tools for niche purposes only!
*/

/// A Resource that represents Write access to the Input channel. 
/// 
/// Intended for mocking purposes - NOT for production use!
#[derive(Resource)]
pub(crate) struct ApiInputChannelMock {
    pub(crate) sender: crossbeam_channel::Sender<ApiInMsg>
}


/// A Resource that represents Read access to the Output channel. 
/// 
/// Intended for 'plunger' Systems that deal with clogged Output pipes.
#[derive(Resource)]
pub(crate) struct ApiOutputChannelMaintenance {
    pub(crate) receiver: crossbeam_channel::Receiver<ApiOutMsg>
}


/// A System that handles receiving and applying updates from the user application to the Cranium app.
fn process_input_messages(
    in_channel: ResMut<ApiInputChannel>, 
    out_channel: ResMut<ApiOutputChannel>,
) {
    in_channel.receiver.try_iter().enumerate().for_each(|(i, msg)| {
        bevy::log::debug!("Received input message {:?} - {:?}", i, msg);
        match msg {
            ApiInMsg::Ping => {
                let resp = out_channel.sender.send(ApiOutMsg::Pong);
                match resp {
                    Ok(_) => bevy::log::debug!("Sent back a Pong"),
                    Err(e) => bevy::log::error!("Error sending a Pong: {:?}", e)
                }
            },
        };
    });
}


/// A maintenance system that tries to save clogged output channels by popping oldest messages off of it. 
fn check_output_channel_for_clogs(
    out_channel: ResMut<ApiOutputChannelMaintenance>,
    mut channel_full_ticks: Local<usize>,
) {
    let is_full = out_channel.receiver.capacity().unwrap_or_default() > 0 && out_channel.receiver.is_full();
    if is_full {
        *channel_full_ticks = channel_full_ticks.saturating_add(1);
    }

    if *channel_full_ticks > MAX_FULL_TICKS_FOR_MAINTENANCE {
        // Pop a message from the channel to hopefully unclog it.
        bevy::log::debug!("Cranium output channel clogged, attempting a receive...");
        let _ = out_channel.receiver.recv();
    }
}

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


        // Wire up the systems processing the channels.
        app.add_systems(PreUpdate, process_input_messages);
        app.add_systems(Last, check_output_channel_for_clogs);
    }
}
