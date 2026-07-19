use core::str::Utf8Error;

use cranium_core::bevy::platform::{prelude::String, sync::Arc};

use crate::{FFIRawString, FFISafeInString};

/// Marks a type as unsafely fallibly FFI-readable for Cranium, 
/// i.e. its FFI representation may be convertable to 
/// its Cranium representation of the specified output type T.
/// 
/// The implementation of the conversion may be unsafe; 
/// users are expected to prove safety invariants at each usage site. 
pub unsafe trait UnsafelyMaybeFFIReadable<T, E> {
    unsafe fn try_ffi_read_unsafe(self) -> Result<T, E>;
}

/// Marks a type as safely fallibly FFI-readable for Cranium, 
/// i.e. its FFI representation may be convertable to 
/// its Cranium representation of the specified output type T.
/// 
/// This trait implies a blanket implementation for UnsafelyMaybeFFIReadable<Self, ()> 
/// (where the unsafe implementation is not REALLY unsafe) for convenience/uniformity.
pub trait MaybeFFIReadable<T, E> {
    fn try_ffi_read(self) -> Result<T, E>;
}

/// Marks a type as infallibly FFI-readable for Cranium, 
/// i.e. its FFI representation can always be safely converted to 
/// its Cranium representation of the specified output type T.
/// 
/// This trait implies a blanket implementation for 
/// MaybeFFIReadable<Self, ()> (no-op wrapped in an Ok variant)
pub trait FFIReadable<T> {
    fn ffi_read(self) -> T;
}

/// Marks a type as trivially FFI-readable for Cranium, 
/// i.e. its FFI representation is the same as its Cranium representation proper.
/// 
/// This is the case for any primitive type like u8, bool, or i64, 
/// and for any types where we use the C layout verbatim.
/// 
/// Note that this trait is NOT guaranteed to be implemented for all eligible types; 
/// the impls may be expanded over time on an as-needed basis.
/// 
/// This trait implies blanket implementations for FFIReadable<Self> (no-op)
/// and MaybeFFIReadable<Self, ()> (no-op wrapped in an Ok variant)
pub trait TriviallyFFIReadable {}


impl<T: TriviallyFFIReadable> FFIReadable<T> for T {
    // TriviallyFFIReadable implies FFIReadable for Self
    fn ffi_read(self) -> Self {
        self
    }
}

impl<P, T: FFIReadable<P>> MaybeFFIReadable<P, ()> for T {
    // FFIReadable implies MaybeFFIReadable for Result<Self, ()>
    // (the Err type here should really be `!`, but this is not yet stable)
    fn try_ffi_read(self) -> Result<P, ()> {
        Ok(self.ffi_read())
    }
}

unsafe impl<P, E, T: MaybeFFIReadable<P, E>> UnsafelyMaybeFFIReadable<P, E> for T {
    // MaybeFFIReadable implies UnsafelyMaybeFFIReadable where the unsafe fn... isn't 
    // (i.e., the safety invariants for the unsafe block are ALWAYS satisfied).
    unsafe fn try_ffi_read_unsafe(self) -> Result<P, E> {
        self.try_ffi_read()
    }
}


impl<'a> FFIReadable<Arc<String>> for FFISafeInString<'a> {
    fn ffi_read(self) -> Arc<String> {
        crate::safer_ingest_string_from_ffi_raw_string(self)
    }
}

unsafe impl UnsafelyMaybeFFIReadable<Arc<String>, Utf8Error> for FFIRawString {
    unsafe fn try_ffi_read_unsafe(self) -> Result<Arc<String>, Utf8Error> {
        unsafe { 
            crate::try_ingest_string_from_ffi_raw_string(self) 
        }
    }
}

/// Marks a type as safely FFI-writeable for Cranium, 
/// i.e. its Cranium representation may be convertable to 
/// its C FFI representation of the specified output type T.
/// 
/// The dual path of the assorted FFIReadable(s).
pub trait FFIOutputtable<T> {
    fn to_ffi_output(self) -> T;
}

/// Marks a type as trivially FFI-writeable for Cranium, 
/// i.e. its C FFI representation is the same as its 
/// Cranium representation.
/// 
/// This is the case for any primitive type like u8, bool, or i64, 
/// and for any types where we use the C layout verbatim.
/// 
/// Note that this trait is NOT guaranteed to be implemented for all eligible types; 
/// the impls may be expanded over time on an as-needed basis.
pub trait TriviallyFFIOutputtable {}

impl<P: TriviallyFFIOutputtable> FFIOutputtable<P> for P {
    fn to_ffi_output(self) -> P {
        self
    }
}

/// Marks a type as both trivially FFI-readable and FFI-writeable, 
/// i.e. its C FFI representation is the same as its 
/// Cranium representation BOTH WAYS (input and output).
/// 
/// This is the case for any primitive type like u8, bool, or i64, 
/// and for any types where we use the C layout verbatim.
/// 
/// Note that this trait is NOT guaranteed to be implemented for all eligible types; 
/// the impls may be expanded over time on an as-needed basis.
/// 
/// This is likely to be the most common trait used, but there may be cases where 
/// just the Input or Output types in FFI require extra work to be constructed, which 
/// is why we need the added distinction this trait provides. 
pub trait FullyFFITransparent {}

// By definition, FullyFFITransparent implies both trivial Input and Output impls:
impl<T: FullyFFITransparent> TriviallyFFIReadable for T {}
impl<T: FullyFFITransparent> TriviallyFFIOutputtable for T {}

/* Impls for trivially representable types */
// Unsigned integers
impl FullyFFITransparent for u8 {}
impl FullyFFITransparent for u16 {}
impl FullyFFITransparent for u32 {}
impl FullyFFITransparent for u64 {}
impl FullyFFITransparent for u128 {}
// Signed integers
impl FullyFFITransparent for i8 {}
impl FullyFFITransparent for i16 {}
impl FullyFFITransparent for i32 {}
impl FullyFFITransparent for i64 {}
impl FullyFFITransparent for i128 {}
// Floats
// impl FullyFFITransparent for f16 {}
impl FullyFFITransparent for f32 {}
impl FullyFFITransparent for f64 {}
// impl FullyFFITransparent for f128 {}
// Others
impl FullyFFITransparent for bool {}
