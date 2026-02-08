/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
//! This crate provides interfaces for Cranium to talk to the outside world. 
//! 
//! It is primarily meant as a companion for the cranium-api crate that actually does the talking; 
//! it exists as its own crate to allow users to only import the types (for instantiation or binding) 
//! without rebuilding the whole API mechanisms itself.

#![no_std]

mod types;
pub use types::*;
