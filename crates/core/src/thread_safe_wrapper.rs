/* 
This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. 
If a copy of the MPL was not distributed with this file, 
You can obtain one at https://mozilla.org/MPL/2.0/. 
*/
//! An abstraction over whatever thread-safe shared pointer type (i.e. Arc<T>-like) 
//! the library has decided to use. 
//! 
//! This is used by Cranium for any possibly-parallel scenario where we need 
//! cheap clone()s or dynamic dispatch, e.g. for registering user `Systems`.
//! 
//! The backing library's datatype should be treated as a hidden implementation detail 
//! for the overwhelming majority of possible purposes.
use bevy::platform::sync::Arc;

/// The underlying 'backend' type for ThreadSafeRef
type ThreadSafeRefValue<T> = Arc<T>;

/// An abstraction over whatever thread-safe shared pointer type (i.e. [`Arc<T>`]-like) 
/// the library has decided to use.
/// 
/// This is used by Cranium for any possibly-parallel scenario where we need 
/// cheap clone()s or dynamic dispatch, e.g. for registering user `Systems`.
/// 
/// The backing library's datatype should be treated as a hidden implementation detail 
/// for the overwhelming majority of possible purposes.
/// 
/// Note that this type does NOT provide weakrefs (e.g. [`alloc::sync::Weak`])! 
/// 
/// Any abstracted weakrefs used by the library will be provided separately, if at all.
#[derive(Debug, Hash, Default, bevy::reflect::Reflect)]
pub struct ThreadSafeRef<T: ?Sized> {
    pub(crate) wrapped: ThreadSafeRefValue<T>
}

impl<T> ThreadSafeRef<T> {
    #[inline]
    pub fn new(val: T) -> Self {
        Self { wrapped: Arc::new(val) }
    }
}

impl<T: ?Sized> ThreadSafeRef<T> {
    #[inline]
    pub fn new_from_ref(val: ThreadSafeRefValue<T>) -> Self {
        Self { wrapped: val }
    }
}

impl<T: ?Sized> Clone for ThreadSafeRef<T> {
    fn clone(&self) -> Self {
        Self::new_from_ref(self.wrapped.clone())
    }
}

impl<T: ?Sized> From<Arc<T>> for ThreadSafeRef<T> {
    fn from(value: Arc<T>) -> Self {
        Self::new_from_ref(value)
    }
}

// Safety: these are thin wrappers over Sync + Send types, so they are Sync + Send too.
unsafe impl<T: ?Sized + Sync + Send> Send for ThreadSafeRef<T> {}
unsafe impl<T: ?Sized + Sync + Send> Sync for ThreadSafeRef<T> {}

impl<T: ?Sized> core::ops::Deref for ThreadSafeRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.wrapped.deref()
    }
}

impl<T: ?Sized> AsRef<Arc<T>> for ThreadSafeRef<T> {
    fn as_ref(&self) -> &Arc<T> {
        &self.wrapped
    }
}

impl<T: PartialEq + ?Sized> PartialEq for ThreadSafeRef<T> {
    fn eq(&self, other: &Self) -> bool {
        self.wrapped == other.wrapped
    }
}

impl<T: Eq + ?Sized> Eq for ThreadSafeRef<T> {}

impl<T: PartialOrd + ?Sized> PartialOrd for ThreadSafeRef<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.wrapped.partial_cmp(&other.wrapped)
    }
}

impl<T: Ord + ?Sized> Ord for ThreadSafeRef<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.wrapped.cmp(&other.wrapped)
    }
}
