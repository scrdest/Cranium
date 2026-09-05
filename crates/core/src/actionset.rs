/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! ActionSets - bundles of Actions, typically all the affordances of a SmartObject or an NPC.

use bevy::prelude::*;
use crate::{actions::ActionTemplate, types::ThreadSafeRef};

#[cfg(feature = "actionset_loader")]
use serde::{Serialize, Deserialize};

/// This is the purely technical bridge between a SerDe-friendly storage format
/// and the more efficient runtime representation (using fat pointers etc.)
#[derive(Debug, Clone, Reflect)]
#[cfg_attr(
    any(feature = "actionset_loader"), 
    derive(Serialize, Deserialize, Asset), 
    serde(from = "ActionSetRaw", into = "ActionSetRaw")
)]
pub struct ActionSetRaw {
    name: String,
    actions: crate::types::CraniumList<ActionTemplate>,
}

#[derive(Debug, Clone, Reflect)]
#[cfg_attr(
    any(feature = "actionset_loader"), 
    derive(Serialize, Deserialize, Asset), 
    serde(from = "ActionSetRaw", into = "ActionSetRaw")
)]
pub struct ActionSet {
    pub name: String,
    pub actions: crate::types::CraniumList<ThreadSafeRef<ActionTemplate>>,
}

impl ActionSet {
    /// Construct an ActionSet from a name and a list of ActionTemplate values (refcounted)
    pub fn new<IS: Into<String>>(name: IS, actions: crate::types::CraniumList<ThreadSafeRef<ActionTemplate>>) -> Self {
        Self {
            name: name.into(),
            actions: actions,
        }
    }

    /// Construct an ActionSet from a name and a list of raw ActionTemplate values (non-refcounted, owned)
    pub fn new_raw<IS: Into<String>>(name: IS, actions: crate::types::CraniumList<ActionTemplate>) -> Self {
        Self {
            name: name.into(),
            actions: actions.into_iter().map(|act| ThreadSafeRef::new_from_ref(act.into())).collect(),
        }
    }
}

impl From<ActionSetRaw> for ActionSet {
    fn from(value: ActionSetRaw) -> Self {
        Self {
            name: value.name,
            actions: value.actions.into_iter().map(|act| ThreadSafeRef::new_from_ref(act.into())).collect()
        }
    }
}

impl From<ActionSet> for ActionSetRaw {
    fn from(value: ActionSet) -> Self {
        Self {
            name: value.name,
            actions: value.actions.into_iter().map(|act| act.wrapped.as_ref().clone()).collect()
        }
    }
}
