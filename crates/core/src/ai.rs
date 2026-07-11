/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! The main Component that marks an Entity as running AI calculations.

use bevy::prelude::*;


/// The AIController is the main 'something running AI calculations' marker. 
#[derive(Component, Default, Reflect)]
#[reflect(Component)]
pub struct AIController {}

