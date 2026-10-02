//! Basic marker types that allow you to store essential types (integers from 8- to 128-bit, 
//! floats both single and double precision, strings, etc.) in ReflectMaps. 
//! 
//! All markers defined here use a "Cranium" prefix as the NAME to minimize the risk of 
//! collisions with any custom types you may want to define in your own code.
//! 
//! This is intended as a convenience to save you time on boilerplate marker definitions; 
//! it is NOT guaranteed to be exhaustive, and in particular, Bevy's `Entity` type is very 
//! deliberately not included - ReflectMaps can be serialized and deserialized, and using 
//! Entity in this scenario is a potential footgun, so having to define this yourself is 
//! a guardrail against users who may not understand the risks.
//! 
//! The interface of the ReflectMapTypeMarker trait you need to implement for any custom type 
//! is (by design) very simple, so if you need anything added, you should be able to do so without 
//! much hassle. Just make sure you register the added marker type with your app (the easiest way 
//! to do that being the app.register_map_marker::<T>() added by the RegisterReflectMapMarker trait).
//! 
//! This is exposed as a Bevy Plugin, so you can opt-out of having these registered if, 
//! for whatever reason, you do not want to have them defined in your application.

use bevy::app::{App, Plugin};
use bevy::{platform::prelude::*, reflect::TypePath};
use crate::reflectmap::registry::RegisterReflectMapMarker;
use crate::reflectmap::runtime::ReflectMapTypeMarker;
use crate::types::*;

macro_rules! cranium_builtin_simple_type_marker {
    ($typename:ident, $ty:ty) => {
        #[doc = concat!(
            "A ReflectMap type marker indicating the associated stored value is of type [`", 
            stringify!($ty),
            "`]. Its [`ReflectMapMarkerRegistry`] name is `",
            concat!("Cranium", stringify!($typename)),
            "`. ",
        )]
        #[derive(Clone, TypePath)]
        pub struct $typename;

        impl ReflectMapTypeMarker for $typename {
            type Value = $ty;
            const NAME: &'static str = concat!("Cranium", stringify!($typename));
        }
    }
}

// Generate markers for basic Rust types
cranium_builtin_simple_type_marker!{UnitMarker, ()}
cranium_builtin_simple_type_marker!{BoolMarker, bool}
cranium_builtin_simple_type_marker!{U8Marker, u8}
cranium_builtin_simple_type_marker!{I8Marker, i8}
cranium_builtin_simple_type_marker!{U16Marker, u16}
cranium_builtin_simple_type_marker!{I16Marker, i16}
cranium_builtin_simple_type_marker!{U32Marker, u32}
cranium_builtin_simple_type_marker!{I32Marker, i32}
cranium_builtin_simple_type_marker!{U64Marker, u64}
cranium_builtin_simple_type_marker!{I64Marker, i64}
cranium_builtin_simple_type_marker!{U128Marker, u128}
cranium_builtin_simple_type_marker!{I128Marker, i128}
cranium_builtin_simple_type_marker!{UsizeMarker, usize}
cranium_builtin_simple_type_marker!{IsizeMarker, isize}
cranium_builtin_simple_type_marker!{F32Marker, f32}
cranium_builtin_simple_type_marker!{F64Marker, f64}
cranium_builtin_simple_type_marker!{StringMarker, String}
cranium_builtin_simple_type_marker!{CraniumCowStrMarker, CraniumCow<'static, str>}


/// A [`Plugin`] that registers a wide array of [`ReflectMapTypeMarker`]s for 
/// common simple Rust types for you into your [`App`].
/// 
/// This includes all stable signed and unsigned integers including [`usize`]/[`isize`], 
/// [`f32`] and [`f64`], [`bool`], [`()`], [`CraniumCow`] strings and regular [`String`]s.
/// 
/// To use, simply add with [`App::add_plugins()`] in your Bevy app build 
/// (or inside another Bevy [`Plugin`]).
pub struct BasicReflectTypemarkersPlugin;

impl Plugin for BasicReflectTypemarkersPlugin {
    fn build(&self, app: &mut App) {
        app
        .register_map_marker::<UnitMarker>()
        .register_map_marker::<BoolMarker>()
        .register_map_marker::<U8Marker>()
        .register_map_marker::<I8Marker>()
        .register_map_marker::<U16Marker>()
        .register_map_marker::<I16Marker>()
        .register_map_marker::<U32Marker>()
        .register_map_marker::<I32Marker>()
        .register_map_marker::<U64Marker>()
        .register_map_marker::<I64Marker>()
        .register_map_marker::<U128Marker>()
        .register_map_marker::<I128Marker>()
        .register_map_marker::<UsizeMarker>()
        .register_map_marker::<IsizeMarker>()
        .register_map_marker::<F32Marker>()
        .register_map_marker::<F64Marker>()
        .register_map_marker::<StringMarker>()
        .register_map_marker::<CraniumCowStrMarker>()
        ;
    }
}

