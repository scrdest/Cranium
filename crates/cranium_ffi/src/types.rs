/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

use crate::{host_id::NativeHostIdType};

// We need this import or Cargo complains about panic unwinding vOv
use bevy::prelude::*;

/// The structure of a single Component upsert request. 
/// Contains the identifier of the Component type and arbitrarily serialized field values.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct FFIComponentPayload {
    fq_path: String, 
    data: Vec<u8>,
}


/// A repr(C) enum of all possible state sync operations (Host -> Cranium only!)
#[repr(C)]
#[derive(Debug, Clone)]
pub enum EntityOperation {
    UpsertEntity {
        host_id: NativeHostIdType,
        components: String
    },

    RemoveEntity {
        host_id: NativeHostIdType
    }
}


/// A repr(C) enum of all possible messages Cranium can receive from the outside world.
#[repr(C)]
#[derive(Debug)]
pub enum ApiInMsg {
    /// A request to terminate the server.
    Shutdown, 

    /// Minimal probe to check if the server is responsive.
    Ping,

    /// [`EntityOperation`]s, batched together for efficiency. The workhorse variant of this enum.
    SyncBatch {
        ops: Vec<EntityOperation>,
    },

    /// Triggers Cranium to run AI decision processing for specified target Entities (by Host ID).
    RequestDecision {
        targets: Vec<(String, NativeHostIdType)>,
    },

}


/// A repr(C) enum of all possible things Cranium will message about externally. 
#[repr(C)]
#[derive(Debug, Clone)]
pub enum ApiOutMsg {
    /// Server confirms it has gone online.
    CraniumStarted,

    /// Server warns that it is about to shut down. 
    CraniumTerminating,

    /// Response to a Ping request - confirms it's live and responsive.
    Pong,

    /// The core output of Cranium - selected an Action for some AI Agent. 
    ActionChosen {
        host_agent_id: NativeHostIdType, 
        host_action_id: String, // todo: review/replace with something sensible!
        host_context_id: NativeHostIdType, // todo: review/replace with something sensible!
    },

    EntitySpawnSuccessful(NativeHostIdType),
    EntitySpawnError(NativeHostIdType, String)
}


/// A tiny reimplementation of Option<T> with a guaranteed repr(C) 
/// and trivial conversion from/to classic Option<T>.
#[repr(C)]
pub enum FFIOption<T> {
    Some(T),
    None
}

impl<T> FFIOption<T> {
    pub fn is_some(&self) -> bool {
        match self {
            Self::Some(_) => true,
            Self::None => false,
        }
    }

    pub fn is_none(&self) -> bool {
        match self {
            Self::Some(_) => false,
            Self::None => true,
        }
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for FFIOption<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Some(arg0) => f.debug_tuple("Some").field(arg0).finish(),
            Self::None => write!(f, "None"),
        }
    }
}

impl<T> Into<FFIOption<T>> for Option<T> {
    fn into(self) -> FFIOption<T> {
        match self {
            Self::None => FFIOption::None,
            Self::Some(v) => FFIOption::Some(v)
        }
    }
}

impl<T> Into<Option<T>> for FFIOption<T> {
    fn into(self) -> Option<T> {
        match self {
            Self::None => Option::None,
            Self::Some(v) => Option::Some(v)
        }
    }
}
