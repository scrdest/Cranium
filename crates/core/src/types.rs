/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
//! Type aliases and 'abstracting' newtypes.

use bevy::platform::prelude::String;

pub use crate::thread_safe_wrapper::ThreadSafeRef;

#[cfg(all(feature = "std", not(feature = "nostd_support")))]
mod std_types {
    extern crate std;
    
    /// A standardized Cranium type for Read-Write Locks (i.e. std::sync::RwLock<T> or similar)
    pub type CraniumRwLock<T> = bevy::platform::sync::RwLock<T>;

    /// A standardized Cranium type for dynamic arrays (i.e. Vec<T> or equivalents)
    pub type CraniumList<T> = bevy::platform::prelude::Vec<T>;

    /// A standardized Cranium type for key-value maps (generally hashmaps; HashMap or equivalent)
    pub type CraniumKvMap<K, V> = bevy::platform::collections::HashMap<K, V>;
}

#[cfg(all(any(feature = "nostd_support")))]
mod nostd_types {
    //! std-free implementations 
    
    /// A standardized Cranium type for Read-Write Locks (i.e. std::sync::RwLock<T> or similar)
    pub type CraniumRwLock<T> = bevy::platform::sync::RwLock<T>;

    /// A standardized Cranium type for dynamic arrays (i.e. Vec<T> or equivalents)
    pub type CraniumList<T> = bevy::platform::prelude::Vec<T>;

    /// A standardized Cranium type for key-value maps (generally hashmaps; HashMap or equivalent)
    pub type CraniumKvMap<K, V> = bevy::platform::collections::HashMap<K, V>;
}

// If nostd_support is enabled, it takes precedence over std. 
// This is to ensure that libs downstream that build for maybe-no_std 
// using both features don't get a surprise std-only datastructure.
#[cfg(all(feature = "std", not(feature = "nostd_support")))]
pub use std_types::*;  

#[cfg(feature = "nostd_support")]
pub use nostd_types::*;

/// 
pub type ContextFetcherKey = crate::identifiers::ContextFetcherIdentifier;
pub type UtilityCurveKey = String;

/// Type alias to make it easier to switch out what datatypes are used for Actions. 
/// Action Keys are effectively IDs, so they do not need to be human-readable.
pub type ActionKey = String;

pub type ActionScore = f32;
pub use crate::action_state::ActionState;

pub const MIN_CONSIDERATION_SCORE: ActionScore = 0.;
pub const MAX_CONSIDERATION_SCORE: ActionScore = 1.;

pub type ActionTemplate = crate::actions::ActionTemplate;
pub type ActionTemplateRef = ThreadSafeRef<ActionTemplate>;

pub type ActionContext = crate::actions::ActionContext;
pub type ActionContextRef = ActionContext; // currently Entity, which is Copy and serves as a reference copied.
pub type ActionContextList = CraniumList<ActionContextRef>;

// Type aliases - to express intent better.
pub type AiEntity = bevy::prelude::Entity;
pub type PawnEntity = bevy::prelude::Entity;
pub type PawnEntityRef = Option<PawnEntity>;

pub use crate::context_fetchers::ContextFetcherInputs;
pub use crate::context_fetchers::ContextFetcherOutputs;
pub use crate::context_fetchers::ContextFetcherSystem;
pub use crate::context_fetchers::IntoContextFetcherSystem;

pub use crate::considerations::ConsiderationInputs;
pub use crate::considerations::ConsiderationOutputs;
pub use crate::considerations::ConsiderationSystem;
pub use crate::considerations::IntoConsiderationSystem;

pub type SmartObjects = crate::smart_object::SmartObjects;

pub type ActionSetRef = String;
pub type ActionSetsRef = ThreadSafeRef<CraniumList<ActionSetRef>>;

pub type EntityIdentifier = crate::entity_identifier::EntityIdentifier;

pub type AiLodLevelPrimitive = u8;


/// A repr(C) enum of all possible messages Cranium can receive from the outside world.
#[repr(C)]
#[derive(Debug)]
pub enum ApiInMsg {
    Ping,
}


/// A repr(C) enum of all possible things Cranium will message about externally. 
#[repr(C)]
#[derive(Debug)]
pub enum ApiOutMsg {
    CraniumStarted,
    Pong,
}


/// A tiny reimplementation of Option<T> with a guaranteed repr(C) 
/// and trivial conversion from/to classic Option<T>.
#[repr(C)]
pub enum FFIOption<T> {
    Some(T),
    None
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
