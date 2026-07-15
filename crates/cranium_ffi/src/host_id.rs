/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/

//! Mappings between 

use core::fmt::Debug;

use bevy::{prelude::*};

/// Host ID is the identifier of some entity in third-party software calling INTO Cranium. 
/// From the Host's PoV, this should be a stable key, even if the underlying captial-E Entity 
/// changes on the Cranium side for whatever reason (e.g. an update implemented as delete-and-recreate).
pub trait HostIdType: Send + Sync + Eq + core::hash::Hash + Clone + Debug {
    type ValueType;

    fn from_value(value: Self::ValueType) -> Self;

    fn to_value(self) -> Self::ValueType;
}

/// Helper trait that marks the implementing type as a trivially constructable Host ID 
/// that acts as a transparent(ish), isomorphic wrapper for the underlying implementor.
/// 
/// All types implementing TrivialHostIdType blanket-implement the TrivialHostIdType.
pub trait TrivialHostIdType: Send + Sync + Eq + core::hash::Hash + Clone + Debug {}

impl<T: TrivialHostIdType> HostIdType for T {
    type ValueType = T;

    fn from_value(value: Self::ValueType) -> Self {
        value
    }

    fn to_value(self) -> Self::ValueType {
        self
    }
}

impl TrivialHostIdType for String {}
impl TrivialHostIdType for u16 {}
impl TrivialHostIdType for u32 {}
impl TrivialHostIdType for u64 {}
impl TrivialHostIdType for usize {}


/// HostIdTypes that Cranium supports out of the box for the C API (DLLs)
/// Additional ones may be added by implementing the HostIdType Trait.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum NativeHostIdType {
    U64(u64),
    U32(u32),
    I64(i64),
    I32(i32),
}

impl NativeHostIdType {
    fn try_as_u64(self) -> Option<u64> {
        match self {
            Self::U64(v) => Some(v),
            _ => None
        }
    }

    fn try_as_u32(self) -> Option<u32> {
        match self {
            Self::U32(v) => Some(v),
            _ => None
        }
    }

    fn try_as_i64(self) -> Option<i64> {
        match self {
            Self::I64(v) => Some(v),
            _ => None
        }
    }

    fn try_as_i32(self) -> Option<i32> {
        match self {
            Self::I32(v) => Some(v),
            _ => None
        }
    }
}

impl From<u64> for NativeHostIdType {
    fn from(value: u64) -> Self {
        Self::U64(value)
    }
}

impl TryInto<u64> for NativeHostIdType {
    type Error = ();

    fn try_into(self) -> Result<u64, Self::Error> {
        self.try_as_u64().ok_or(())
    }
}

impl From<u32> for NativeHostIdType {
    fn from(value: u32) -> Self {
        Self::U32(value)
    }
}

impl TryInto<u32> for NativeHostIdType {
    type Error = ();

    fn try_into(self) -> Result<u32, Self::Error> {
        self.try_as_u32().ok_or(())
    }
}

impl From<i64> for NativeHostIdType {
    fn from(value: i64) -> Self {
        Self::I64(value)
    }
}

impl TryInto<i64> for NativeHostIdType {
    type Error = ();

    fn try_into(self) -> Result<i64, Self::Error> {
        self.try_as_i64().ok_or(())
    }
}

impl From<i32> for NativeHostIdType {
    fn from(value: i32) -> Self {
        Self::I32(value)
    }
}

impl TryInto<i32> for NativeHostIdType {
    type Error = ();

    fn try_into(self) -> Result<i32, Self::Error> {
        self.try_as_i32().ok_or(())
    }
}


impl TrivialHostIdType for NativeHostIdType {}


/// A Bevy Component that indicates the holding Entity is a proxy for an External Thing of some kind 
/// (e.g. if the Host is an ECS engine, this would be another world's Entity; if OOP - it could be an object).
/// 
/// We will assume that the mapping is one-to-many, i.e. the Host has a single unique HostId on their side, 
/// and Cranium has zero or more Entities with a HostMapped Component holding that HostId. 
// 
// -- TECHNICAL NOTES (not part of API docs) --
// 
// If a HostId is held by zero Entities, then this object is 'novel', i.e. we hadn't seen it before; 
// generally that indicates an error somewhere - either the client forgot to register that HostId, 
// or we somehow forgot about it existing (e.g. if we ever do GC-style cleanup of stale proxies).
// 
// If a HostId is held by exactly one Cranium Entity, this is the ideal, hopefully-usual scenario.
// 
// If there is more than one Entity matching this HostId, we have redundancy; this will almost 
// always be an issue on the Cranium side and may result in some flavor of cleanup internally 
// where detected.
#[derive(Component)]
pub struct HostMapped<T: HostIdType> {
    /// The HostId this Entity ID maps to.
    pub host_id: T,

    /// The logical creation time for this mapping; used for deduplication.
    pub creation_time: core::time::Duration,
}


impl<T: HostIdType> HostMapped<T> {
    /// Creates a new mapping with the specified creation time 
    /// (broadly, should correspond to 'now' in the real world). 
    /// 
    /// If the creation time is None, this mapping will be treated as 'primordial', 
    /// i.e. created at the dawn of the universe. In case of HostId clashes, this 
    /// will result in any values created with a real creation time to take precedence. 
    /// 
    /// Doesn't require access to a Time<Real> resource, but requires some user caution 
    /// when choosing values to not create an unholy mess in Cranium.
    pub fn from_value_at_time(value: T, creation_time: Option<core::time::Duration>) -> Self {
        Self {
            host_id: value.into(),
            creation_time: creation_time.unwrap_or(core::time::Duration::ZERO),
        }
    }

    /// Creates a new mapping with creation time inferred from the provided Time resource.
    /// 
    /// Requires access to a Time<Real> resource, which may be annoying in some setups like 
    /// unit-tests, but broadly recommended as the default for most applications.
    pub fn from_value_now(value: T, real_timer: Res<Time<Real>>) -> Self {
        Self {
            host_id: value.into(),
            creation_time: real_timer.elapsed(),
        }
    }
}


impl<T: HostIdType> AsRef<T> for HostMapped<T> {
    fn as_ref(&self) -> &T {
        &self.host_id
    }
}
