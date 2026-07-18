/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
#![no_std]

/// This is a re-export of Bevy to allow other crates to link to whatever the core is built around.
/// This is not really meant to be public, but as of writing there was no nice visibility for this purpose. 
pub use bevy;

pub mod ai;
pub mod actions;
pub mod actionset;
pub mod action_runtime;
pub mod action_state;
pub mod considerations;
pub mod context_fetchers;
pub mod curves;
// pub mod brain;
pub mod decision_loop;
pub mod errors;
pub mod entity_identifier;
pub mod events;
pub mod identifiers;
pub mod lods;
// pub mod memories;
pub mod pawn;
// pub mod senses;
pub mod smart_object;
mod thread_safe_wrapper;
pub mod types;
