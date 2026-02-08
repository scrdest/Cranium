// We need this import or Cargo complains about panic unwinding vOv
use bevy::prelude::*;


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
