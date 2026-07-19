/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
use core::{ffi::CStr, fmt::Display, str::Utf8Error};

use crate::{TriviallyFFIReadable, host_id::NativeHostIdType};

// We need this import or Cargo complains about panic unwinding vOv
use cranium_core::bevy::{platform::sync::Arc, prelude::*};

pub type RequestKey = cranium_core::types::RequestKey;

/// Alias for the standard Cranium representation of string data across FFI boundaries. 
/// This is the thing Cranium's DLL/SO interfaces consume directly.
/// 
/// Under the hood, this is a raw C-style string, represented as a pointer. 
pub type FFIRawString = *const core::ffi::c_char;

/// Alias for the standard Cranium representation of FFI string data safely 
/// shepherded into the safe confines of Cranium's own invariants.
pub type FFIIngestedString = Arc<String>;

pub type FFISafeInString<'a> = safer_ffi::prelude::char_p::Ref<'a>;
pub type FFISafeOutString = safer_ffi::string::String;

/// Alias for an FFI-friendly Vec representation.
pub type FFIVec<T> = safer_ffi::vec::Vec<T>;

#[repr(C)]
#[derive(Debug, Clone)]
pub struct FFISpawnRequest<I: Into<NativeHostIdType> + TriviallyFFIReadable + Clone> {
    pub host_id: I,
    pub components: FFIRawString,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct FFIDespawnRequest<I: Into<NativeHostIdType> + TriviallyFFIReadable + Clone> {
    pub host_id: I,
}

pub type FFISpawnRequestBatch<T> = FFIVec<FFISpawnRequest<T>>;
pub type FFIDespawnRequestBatch<T> = FFIVec<FFIDespawnRequest<T>>;


/// Turns a normal Rust &str into Cranium's FFI-friendly representation.
/// 
/// IMPORTANT: The input string should contain at least one nul 
/// (i.e. '\0', unquoted) at the end to fit the expected format.
/// 
/// Because of this restriction, the function is unsafe - failing to 
/// ensure the input is nul-terminated will result in a panic!
///  
/// Primarily intended as a convenience for testing FFI calls from Rust 
/// because it's a pretty gnarly piece of boilerplate in those scenarios. 
pub unsafe fn ffi_raw_string_from_str_unchecked(inp: &str) -> FFIRawString {
    core::ffi::CStr::from_bytes_until_nul(
        inp.as_bytes()
    )
    .map_err(|err| {
        #[cfg(feature = "logging")]
        cranium_core::bevy::log::error!("Error converting Rust string {} to FFI string - {}", inp, err)
    })
    .map(|s| s.as_ptr())
    .unwrap()
}

/// Turns a normal Rust &str into Cranium's FFI-friendly representation.
///  
/// Primarily intended as a convenience for testing FFI calls from Rust 
/// because it's a pretty gnarly piece of boilerplate in those scenarios. 
pub fn ffi_raw_string_from_str(inp: &str) -> FFISafeOutString {
    FFISafeOutString::from(inp)
}

/// Fallibly turns the FFIRawString into something slightly more Rust-palatable. 
/// 
/// This is intended purely for internal use for abstraction and boilerplate reduction.
pub unsafe fn try_ingest_string_from_ffi_raw_string<'a>(inp: FFIRawString) -> Result<FFIIngestedString, Utf8Error> {
    // We copy this over to an owned String ASAP to make sure nobody can mess up the data 
    // the pointer is pointer-ing to; after that point, we have ensured a nice safe value.
    unsafe { core::ffi::CStr::from_ptr(inp) }.to_str().map(|s| Arc::new(s.to_string())) }


/// This is a safer_ffi-backed variant of ingest_string_from_ffi_raw_string. 
/// Unlike its non-safer_ffi sibling, it is *entirely* safe.
pub fn safer_ingest_string_from_ffi_raw_string<'a>(inp: FFISafeInString) -> FFIIngestedString {
    // We copy this over to an owned String ASAP to make sure nobody can mess up the data 
    // the pointer is pointer-ing to; after that point, we have ensured a nice safe value.
    Arc::new(inp.to_string())
}

pub fn safe_output_string_from_rust_string(inp: String) -> FFISafeOutString {
    FFISafeOutString::from(inp)
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

impl<T> From<Option<T>> for FFIOption<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            None => FFIOption::None,
            Some(v) => FFIOption::Some(v)
        }
    }
}

impl<T> From<FFIOption<T>> for Option<T> {
    fn from(value: FFIOption<T>) -> Self {
        match value {
            FFIOption::None => Option::None,
            FFIOption::Some(v) => Option::Some(v)
        }
    }
}


/// An enum of all possible state sync operations (Host -> Cranium only!)
#[derive(Debug, Clone)]
pub enum EntityOperation {
    UpsertEntity {
        host_id: NativeHostIdType,
        components: Arc<String>, 
        request_key: RequestKey,
    },

    RemoveEntity {
        host_id: NativeHostIdType, 
        request_key: RequestKey,
    }
}


/// An enum of all possible messages Cranium can receive from the outside world.
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
        targets: Vec<(RequestKey, NativeHostIdType)>,
    },
}

/// A repr(C) enum of all possible things Cranium will message about externally. 
/// 
/// This is NOT what the actual Channels carry in Rust, 
/// but it IS what Cranium outputs to users.
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
        request_key: RequestKey,
    },

    /// Feedback message sent if ActionChosen is infeasible for any reason 
    /// (e.g. the entity does not exist, is not AI-enabled, or has nothing to do)
    NoActionChosen {
        host_agent_id: NativeHostIdType, 
        request_key: RequestKey,
        comment: FFIOption<FFIRawString>, 
    },

    // Feedback for Spawn ops
    EntitySpawnSuccessful(NativeHostIdType, RequestKey),
    EntitySpawnError(NativeHostIdType, RequestKey, FFIRawString), 

    // Feedback for Despawn ops
    EntityDespawnSuccessful(FFIOption<NativeHostIdType>, RequestKey),
    EntityDespawnError(FFIOption<NativeHostIdType>, RequestKey, FFIRawString),
}

impl Display for ApiOutMsg {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EntitySpawnError(id, rqkey, err) => {
                f.write_str(&format!(
                    "EntitySpawnError(HostId: {:?}, RequestKey: {:?}, Error: '{}')",
                    id, rqkey, unsafe { CStr::from_ptr(*err) }.to_string_lossy()
                ))
            },
            Self::EntityDespawnError(id, rqkey, err) => {
                f.write_str(&format!(
                    "EntityDespawnError(HostId: {:?}, RequestKey: {:?}, Error: '{}')",
                    id, rqkey, unsafe { CStr::from_ptr(*err) }.to_string_lossy()
                ))
            },
            Self::NoActionChosen { host_agent_id, request_key, comment } => {
                f.write_str(&format!(
                    "NoActionChosen(HostId: {:?}, RequestKey: {:?}, Comment: '{}')",
                    host_agent_id, 
                    request_key, 
                    match comment {
                        FFIOption::Some(err) => unsafe { CStr::from_ptr(*err) }.to_str().unwrap_or_default(),
                        FFIOption::None => "<none>"
                    }
                ))
            },
            _ => f.write_str(&format!("{:?}", self)),
        }
    }
}



/// This is an 'unbaked' ApiOutMsg that can be used in Messages, but requires FFI-zation. 
/// Critically, this type is NOT repr(C) and is not trying to be. 
/// Types may be mapped into C-friendly types when converting this to an ApiOutMsg.
#[derive(Debug, Clone)]
pub enum StagedApiOutMsg {
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
        request_key: RequestKey,
    },

    /// Feedback message sent if ActionChosen is infeasible for any reason 
    /// (e.g. the entity does not exist, is not AI-enabled, or has nothing to do)
    NoActionChosen {
        host_agent_id: NativeHostIdType, 
        request_key: RequestKey,
        comment: Option<Arc<String>>,
    },

    // Feedback for Spawn ops
    EntitySpawnSuccessful(NativeHostIdType, RequestKey),
    EntitySpawnError(NativeHostIdType, RequestKey, Arc<String>), 

    // Feedback for Despawn ops
    EntityDespawnSuccessful(FFIOption<NativeHostIdType>, RequestKey),
    EntityDespawnError(FFIOption<NativeHostIdType>, RequestKey, Arc<String>), 
}

impl StagedApiOutMsg {
    /// Retrieves a string FFI message from a variant (if applicable and present) 
    /// for the purposes of stashing it into a resource until the consumer officially 
    /// confirms it's been received.
    pub fn get_message_for_stashing(&self) -> Option<(FFIIngestedString, RequestKey)> {
        match self {
            Self::NoActionChosen { host_agent_id: _, request_key, comment } => comment.as_ref().map(|s| (s.clone(), *request_key)),
            Self::EntitySpawnError(_id, rqk, err) => Some((err.clone(), *rqk)),
            Self::EntityDespawnError(_id, rqk, err) => Some((err.clone(), *rqk)),
            _ => None,
        }
    }
}

impl Into<ApiOutMsg> for StagedApiOutMsg {
    fn into(self) -> ApiOutMsg {
        match self {
            Self::CraniumStarted => ApiOutMsg::CraniumStarted,
            Self::CraniumTerminating => ApiOutMsg::CraniumTerminating,
            Self::Pong => ApiOutMsg::Pong,
            Self::ActionChosen { 
                host_agent_id, 
                host_action_id, 
                host_context_id, 
                request_key 
            } => {
                ApiOutMsg::ActionChosen { 
                    host_agent_id, 
                    host_action_id, 
                    host_context_id, 
                    request_key: request_key
                }
            },
            Self::NoActionChosen { host_agent_id, request_key, comment } => {
                ApiOutMsg::NoActionChosen { 
                    host_agent_id, 
                    request_key, 
                    comment: match comment {
                        Some(v) => FFIOption::Some(unsafe { 
                            // SAFETY: We only ever call this with nul-terminated inputs.
                            ffi_raw_string_from_str_unchecked(&v) 
                        }),
                        None => FFIOption::None
                    } 
                }
            },
            Self::EntitySpawnSuccessful(id, request_key) => {
                ApiOutMsg::EntitySpawnSuccessful(id, request_key)
            },
            Self::EntitySpawnError(id, request_key, errmsg) => {
                let ffi_string = unsafe { 
                    // SAFETY: All call-sites generating the message pad the strings with NULs.
                    ffi_raw_string_from_str_unchecked(&errmsg) 
                };
                ApiOutMsg::EntitySpawnError(
                    id, 
                    request_key, 
                    ffi_string, 
                )
            },
            Self::EntityDespawnSuccessful(id, request_key) => {
                ApiOutMsg::EntityDespawnSuccessful(id, request_key)
            },
            Self::EntityDespawnError(id, request_key, errmsg) => {
                let ffi_string = unsafe { 
                    // SAFETY: All call-sites generating the message use a nul-terminated static str.
                    ffi_raw_string_from_str_unchecked(&errmsg) 
                };
                ApiOutMsg::EntityDespawnError(
                    id, 
                    request_key, 
                    ffi_string,
                )
            },
        }
    }
}
