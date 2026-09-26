/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

/*! 
This crate extends the Cranium game AI library with a plugin that streamlines the integration 
of Cranium into an existing Bevy application (the "native AI" integration style).

The plugin handles the vast majority of the basic gruntwork - setting up generic Resources, 
Observers and Systems that form the 'framework' part of the AI.

What's left for you to do after adding it in is registering any ContextFetchers, Considerations, 
and any other of your custom Systems that you want Cranium to use
*/

#![no_std]

pub mod ai_plugin;

pub use ai_plugin::CraniumPlugin;
