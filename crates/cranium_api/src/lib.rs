/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
//! This crate provides tooling to run Cranium as an "AI Server" for non-Bevy applications 
//! (or Bevy applications who want to keep their Worlds separate from the AI World).
//! 
//! This includes functions to create an ECS World, drive it tick-by-tick externally, 
//! update the state of the World to keep the AI decisions sane, etc.
#![no_std]

pub(crate) mod api;
pub(crate) mod channels;
pub(crate) mod heartbeat;

pub use api::*;
