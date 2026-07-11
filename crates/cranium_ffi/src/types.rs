/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

use crate::{host_id::NativeHostIdType};

// We need this import or Cargo complains about panic unwinding vOv
use bevy::prelude::*;

/// Alias for the standard Cranium representation of string data across FFI boundaries. 
/// This is the thing Cranium's DLL/SO interfaces consume directly.
/// 
/// Under the hood, this is a raw C-style string, represented as a pointer. 
pub type FFIRawString = *const core::ffi::c_char;

/// Alias for the standard Cranium representation of FFI string data safely 
/// shepherded into the safe confines of Cranium's own invariants.
pub type FFIIngestedString = String;

/// Turns a normal Rust &str into Cranium's FFI-friendly representation.
/// 
/// IMPORTANT: The input string should contain EXACTLY one nul 
/// (i.e. '\0', unquoted) at the end to fit the expected format.
///  
/// Primarily intended as a convenience for testing FFI calls from Rust 
/// because it's a pretty gnarly piece of boilerplate in those scenarios. 
pub fn ffi_raw_string_from_str(inp: &str) -> FFIRawString {
    core::ffi::CStr::from_bytes_with_nul(
        inp.as_bytes()
    )
    .unwrap()
    .as_ptr()
}

/// Turns the FFIRawString into something slightly more Rust-palatable. 
/// 
/// This is intended purely for internal use for abstraction and boilerplate reduction.
pub unsafe fn ingest_string_from_ffi_raw_string<'a>(inp: FFIRawString) -> FFIIngestedString {
    // We copy this over to an owned String ASAP to make sure nobody can mess up the data 
    // the pointer is pointer-ing to; after that point, we have ensured a nice safe value.
    unsafe { core::ffi::CStr::from_ptr(inp) }.to_str().unwrap().to_string()
}


/// A tiny reimplementation of Option<T> with a guaranteed repr(C) 
/// and trivial conversion from/to classic Option<T>.
#[repr(C)]
#[derive(Clone)]
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
        host_action_id: NativeHostIdType, // TODO: review
        host_context_id: NativeHostIdType, // TODO: review
    },

    /// Feedback message sent if ActionChosen is infeasible for any reason 
    /// (e.g. the entity does not exist, is not AI-enabled, or has nothing to do)
    NoActionChosen {
        host_agent_id: NativeHostIdType, 
    },

    // Feedback for Spawn ops
    EntitySpawnSuccessful(NativeHostIdType),
    EntitySpawnError(NativeHostIdType), // TODO: add a way to emit error messages

    // Feedback for Despawn ops
    EntityDespawnSuccessful(FFIOption<NativeHostIdType>),
    EntityDespawnError(FFIOption<NativeHostIdType>), // TODO: add a way to emit error messages
}
